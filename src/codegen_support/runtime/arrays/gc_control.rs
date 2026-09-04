//! Purpose:
//! Emits runtime entry points for PHP cycle-collector controls and status counters.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::managed::emit_managed_runtime()`.
//!
//! Key details:
//! - Automatic safe points honor `_gc_enabled`, while explicit collection bypasses it.
//! - Status reads use one scalar selector shared with the typed EIR `GcControlOp`.
//! - Float metrics return their IEEE-754 bits through the integer result register.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::ir::GcControlOp;

/// Emits target-aware GC enable, disable, query, and automatic-collection wrappers.
pub fn emit_gc_control(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: gc_control ---");
    match emitter.target.arch {
        Arch::AArch64 => emit_gc_control_aarch64(emitter),
        Arch::X86_64 => emit_gc_control_x86_64(emitter),
    }
}

/// Emits the GC control entry points for AArch64 targets.
fn emit_gc_control_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_gc_collect_cycles");
    abi::emit_symbol_address(emitter, "x9", "_gc_enabled");
    emitter.instruction("ldr x9, [x9]");                                        // read whether automatic safe points are enabled
    emitter.instruction("cbz x9, __rt_gc_collect_cycles_disabled");             // skip an automatic collection while disabled
    emitter.instruction("b __rt_gc_collect_cycles_explicit");                   // tail-call the collector without changing its result
    emitter.label("__rt_gc_collect_cycles_disabled");
    emitter.instruction("mov x0, #0");                                          // a skipped automatic pass collects no graph nodes
    emitter.instruction("ret");                                                 // return to the generated safe point

    emitter.label_global("__rt_gc_disable");
    abi::emit_symbol_address(emitter, "x9", "_gc_enabled");
    emitter.instruction("str xzr, [x9]");                                       // disable later automatic collection safe points
    emitter.instruction("mov x0, #0");                                          // materialize the internal void-result placeholder
    emitter.instruction("ret");                                                 // return to the PHP builtin wrapper

    emitter.label_global("__rt_gc_enable");
    abi::emit_symbol_address(emitter, "x9", "_gc_enabled");
    emitter.instruction("mov x10, #1");                                         // prepare the enabled flag
    emitter.instruction("str x10, [x9]");                                       // enable later automatic collection safe points
    emitter.instruction("mov x0, #0");                                          // materialize the internal void-result placeholder
    emitter.instruction("ret");                                                 // return to the PHP builtin wrapper

    emitter.label_global("__rt_gc_enabled");
    abi::emit_load_symbol_to_reg(emitter, "x0", "_gc_enabled", 0);
    emitter.instruction("ret");                                                 // return the current boolean flag

    emit_gc_mem_caches_aarch64(emitter);

    emitter.label_global("__rt_gc_status_metric");
    emitter.instruction(&format!(                                               // compare against the collector-running metric selector
        "cmp x0, #{}",
        GcControlOp::Running.as_i64()
    ));
    emitter.instruction("b.eq __rt_gc_status_running");                         // select the collector-active flag
    emitter.instruction(&format!(                                               // compare against the release-protection metric selector
        "cmp x0, #{}",
        GcControlOp::Protected.as_i64()
    ));
    emitter.instruction("b.eq __rt_gc_status_protected");                       // select release-protection state
    emitter.instruction(&format!("cmp x0, #{}", GcControlOp::Runs.as_i64()));   // compare against the productive-run metric selector
    emitter.instruction("b.eq __rt_gc_status_runs");                            // select the productive-run counter
    emitter.instruction(&format!(                                               // compare against the collected-node metric selector
        "cmp x0, #{}",
        GcControlOp::Collected.as_i64()
    ));
    emitter.instruction("b.eq __rt_gc_status_collected");                       // select the cumulative collected-node counter
    emitter.instruction(&format!("cmp x0, #{}", GcControlOp::Roots.as_i64())); // compare against the live collector-candidate selector
    emitter.instruction("b.eq __rt_gc_status_roots");                           // count current cycle-collector candidates
    emitter.instruction(&format!(                                               // compare against elapsed application time
        "cmp x0, #{}",
        GcControlOp::ApplicationTime.as_i64()
    ));
    emitter.instruction("b.eq __rt_gc_status_application_time");                // calculate live elapsed application time
    emitter.instruction(&format!(                                               // compare against cumulative collector time
        "cmp x0, #{}",
        GcControlOp::CollectorTime.as_i64()
    ));
    emitter.instruction("b.eq __rt_gc_status_collector_time");                  // return stored collector seconds
    emitter.instruction(&format!(                                               // compare against cumulative destructor time
        "cmp x0, #{}",
        GcControlOp::DestructorTime.as_i64()
    ));
    emitter.instruction("b.eq __rt_gc_status_destructor_time");                 // return stored destructor seconds
    emitter.instruction(&format!("cmp x0, #{}", GcControlOp::FreeTime.as_i64())); // compare against cumulative graph-free time
    emitter.instruction("b.eq __rt_gc_status_free_time");                       // return stored graph-free seconds
    emitter.instruction("mov x0, #0");                                          // unknown selectors resolve to integer zero
    emitter.instruction("ret");                                                 // return the unknown-selector fallback
    emitter.label("__rt_gc_status_running");
    abi::emit_load_symbol_to_reg(emitter, "x0", "_gc_collecting", 0);
    emitter.instruction("ret");                                                 // return the collector-active flag
    emitter.label("__rt_gc_status_protected");
    abi::emit_load_symbol_to_reg(emitter, "x0", "_gc_release_suppressed", 0);
    emitter.instruction("ret");                                                 // return release-protection state
    emitter.label("__rt_gc_status_runs");
    abi::emit_load_symbol_to_reg(emitter, "x0", "_gc_runs", 0);
    emitter.instruction("ret");                                                 // return productive collection runs
    emitter.label("__rt_gc_status_collected");
    abi::emit_load_symbol_to_reg(emitter, "x0", "_gc_collected", 0);
    emitter.instruction("ret");                                                 // return cumulative collected graph nodes
    emit_gc_roots_aarch64(emitter);
    emit_gc_timing_aarch64(emitter);
}

/// Emits the GC control entry points for Linux x86_64.
fn emit_gc_control_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_gc_collect_cycles");
    abi::emit_symbol_address(emitter, "r8", "_gc_enabled");
    emitter.instruction("cmp QWORD PTR [r8], 0");                               // read whether automatic safe points are enabled
    emitter.instruction("je __rt_gc_collect_cycles_disabled");                  // skip an automatic collection while disabled
    emitter.instruction("jmp __rt_gc_collect_cycles_explicit");                 // tail-call the collector without changing its result
    emitter.label("__rt_gc_collect_cycles_disabled");
    emitter.instruction("xor eax, eax");                                        // a skipped automatic pass collects no graph nodes
    emitter.instruction("ret");                                                 // return to the generated safe point

    emitter.label_global("__rt_gc_disable");
    abi::emit_symbol_address(emitter, "r8", "_gc_enabled");
    emitter.instruction("mov QWORD PTR [r8], 0");                               // disable later automatic collection safe points
    emitter.instruction("xor eax, eax");                                        // materialize the internal void-result placeholder
    emitter.instruction("ret");                                                 // return to the PHP builtin wrapper

    emitter.label_global("__rt_gc_enable");
    abi::emit_symbol_address(emitter, "r8", "_gc_enabled");
    emitter.instruction("mov QWORD PTR [r8], 1");                               // enable later automatic collection safe points
    emitter.instruction("xor eax, eax");                                        // materialize the internal void-result placeholder
    emitter.instruction("ret");                                                 // return to the PHP builtin wrapper

    emitter.label_global("__rt_gc_enabled");
    abi::emit_load_symbol_to_reg(emitter, "rax", "_gc_enabled", 0);
    emitter.instruction("ret");                                                 // return the current boolean flag

    emit_gc_mem_caches_x86_64(emitter);

    emitter.label_global("__rt_gc_status_metric");
    emitter.instruction(&format!(                                               // compare against the collector-running metric selector
        "cmp rdi, {}",
        GcControlOp::Running.as_i64()
    ));
    emitter.instruction("je __rt_gc_status_running");                           // select the collector-active flag
    emitter.instruction(&format!(                                               // compare against the release-protection metric selector
        "cmp rdi, {}",
        GcControlOp::Protected.as_i64()
    ));
    emitter.instruction("je __rt_gc_status_protected");                         // select release-protection state
    emitter.instruction(&format!("cmp rdi, {}", GcControlOp::Runs.as_i64()));   // compare against the productive-run metric selector
    emitter.instruction("je __rt_gc_status_runs");                              // select the productive-run counter
    emitter.instruction(&format!(                                               // compare against the collected-node metric selector
        "cmp rdi, {}",
        GcControlOp::Collected.as_i64()
    ));
    emitter.instruction("je __rt_gc_status_collected");                         // select the cumulative collected-node counter
    emitter.instruction(&format!("cmp rdi, {}", GcControlOp::Roots.as_i64())); // compare against the live collector-candidate selector
    emitter.instruction("je __rt_gc_status_roots");                             // count current cycle-collector candidates
    emitter.instruction(&format!(                                               // compare against elapsed application time
        "cmp rdi, {}",
        GcControlOp::ApplicationTime.as_i64()
    ));
    emitter.instruction("je __rt_gc_status_application_time");                  // calculate live elapsed application time
    emitter.instruction(&format!(                                               // compare against cumulative collector time
        "cmp rdi, {}",
        GcControlOp::CollectorTime.as_i64()
    ));
    emitter.instruction("je __rt_gc_status_collector_time");                    // return stored collector seconds
    emitter.instruction(&format!(                                               // compare against cumulative destructor time
        "cmp rdi, {}",
        GcControlOp::DestructorTime.as_i64()
    ));
    emitter.instruction("je __rt_gc_status_destructor_time");                   // return stored destructor seconds
    emitter.instruction(&format!("cmp rdi, {}", GcControlOp::FreeTime.as_i64())); // compare against cumulative graph-free time
    emitter.instruction("je __rt_gc_status_free_time");                         // return stored graph-free seconds
    emitter.instruction("xor eax, eax");                                        // unknown selectors resolve to integer zero
    emitter.instruction("ret");                                                 // return the unknown-selector fallback
    emitter.label("__rt_gc_status_running");
    abi::emit_load_symbol_to_reg(emitter, "rax", "_gc_collecting", 0);
    emitter.instruction("ret");                                                 // return the collector-active flag
    emitter.label("__rt_gc_status_protected");
    abi::emit_load_symbol_to_reg(emitter, "rax", "_gc_release_suppressed", 0);
    emitter.instruction("ret");                                                 // return release-protection state
    emitter.label("__rt_gc_status_runs");
    abi::emit_load_symbol_to_reg(emitter, "rax", "_gc_runs", 0);
    emitter.instruction("ret");                                                 // return productive collection runs
    emitter.label("__rt_gc_status_collected");
    abi::emit_load_symbol_to_reg(emitter, "rax", "_gc_collected", 0);
    emitter.instruction("ret");                                                 // return cumulative collected graph nodes
    emit_gc_roots_x86_64(emitter);
    emit_gc_timing_x86_64(emitter);
}

/// Emits the AArch64 allocator-cache drain into the ordered reusable free list.
fn emit_gc_mem_caches_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_gc_mem_caches");
    emitter.instruction("sub sp, sp, #48");                                     // reserve saved registers and the current bin offset
    emitter.instruction("stp x19, x20, [sp, #0]");                             // preserve the cache base and reclaimed-byte accumulator
    emitter.instruction("stp x29, x30, [sp, #16]");                            // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #16");                                   // establish the helper frame pointer
    abi::emit_symbol_address(emitter, "x19", "_heap_small_bins");
    emitter.instruction("mov x20, #0");                                         // initialize reclaimed cache bytes
    emitter.instruction("str xzr, [sp, #32]");                                 // start at the first of four bin slots
    emitter.label("__rt_gc_mem_caches_bin");
    emitter.instruction("ldr x9, [sp, #32]");                                  // reload the current bin byte offset
    emitter.instruction("cmp x9, #32");                                        // have all four bins been drained?
    emitter.instruction("b.eq __rt_gc_mem_caches_done");                        // yes, return the accumulated footprint
    emitter.instruction("add x10, x19, x9");                                   // address the current bin head slot
    emitter.instruction("ldr x9, [x10]");                                      // load the first cached block header
    emitter.instruction("cbz x9, __rt_gc_mem_caches_next_bin");                 // an empty bin needs no work
    emitter.instruction("ldr x12, [x9, #16]");                                 // preserve the cached successor
    emitter.instruction("str x12, [x10]");                                     // unlink this header from the small-bin cache
    emitter.instruction("ldr w11, [x9]");                                      // load its reusable payload capacity
    emitter.instruction("add x20, x20, x11");                                  // include the payload in the released cache total
    emitter.instruction("add x20, x20, #16");                                  // include the uniform allocator header
    emitter.instruction("add x0, x9, #16");                                    // provide the user pointer expected by shared insertion cleanup
    emitter.instruction("bl __rt_heap_free_insert");                           // coalesce this cached block through the ordered free-list path
    abi::emit_symbol_address(emitter, "x10", "_gc_frees");
    emitter.instruction("ldr x11, [x10]");                                     // undo the shared insertion path's historical free count
    emitter.instruction("sub x11, x11, #1");                                   // this block was counted when it first entered the cache
    emitter.instruction("str x11, [x10]");                                     // preserve one free event per released allocation
    emitter.instruction("b __rt_gc_mem_caches_bin");                           // continue draining the same bin
    emitter.label("__rt_gc_mem_caches_next_bin");
    emitter.instruction("ldr x9, [sp, #32]");                                  // reload the completed bin offset
    emitter.instruction("add x9, x9, #8");                                     // advance to the next pointer-sized bin slot
    emitter.instruction("str x9, [sp, #32]");                                  // save the next bin offset
    emitter.instruction("b __rt_gc_mem_caches_bin");                           // drain the next size class
    emitter.label("__rt_gc_mem_caches_done");
    emitter.instruction("mov x0, x20");                                         // return bytes removed from specialized caches
    emitter.instruction("ldp x29, x30, [sp, #16]");                            // restore the caller frame and return address
    emitter.instruction("ldp x19, x20, [sp, #0]");                             // restore callee-saved scratch registers
    emitter.instruction("add sp, sp, #48");                                    // release the helper frame
    emitter.instruction("ret");                                                 // return to the builtin wrapper
}

/// Emits the x86_64 allocator-cache drain into the ordered reusable free list.
fn emit_gc_mem_caches_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_gc_mem_caches");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish stable spill slots across insertion calls
    emitter.instruction("sub rsp, 32");                                         // reserve cache base, total, and bin offset locals
    abi::emit_symbol_address(emitter, "r8", "_heap_small_bins");
    emitter.instruction("mov QWORD PTR [rbp - 8], r8");                        // preserve the small-bin table address
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                        // initialize reclaimed cache bytes
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                        // start at the first bin slot
    emitter.label("__rt_gc_mem_caches_bin");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                      // reload the current bin byte offset
    emitter.instruction("cmp rcx, 32");                                        // have all four bins been drained?
    emitter.instruction("je __rt_gc_mem_caches_done");                          // yes, return the accumulated footprint
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                        // reload the bin table base
    emitter.instruction("lea rdx, [r8 + rcx]");                                // address the current bin head slot
    emitter.instruction("mov r9, QWORD PTR [rdx]");                            // load the first cached block header
    emitter.instruction("test r9, r9");                                         // is this bin empty?
    emitter.instruction("jz __rt_gc_mem_caches_next_bin");                      // yes, advance to the next size class
    emitter.instruction("mov r10, QWORD PTR [r9 + 16]");                       // preserve the cached successor
    emitter.instruction("mov QWORD PTR [rdx], r10");                           // unlink this header from the small-bin cache
    emitter.instruction("mov r11d, DWORD PTR [r9]");                           // load its reusable payload capacity
    emitter.instruction("add QWORD PTR [rbp - 16], r11");                      // include the payload in the released cache total
    emitter.instruction("add QWORD PTR [rbp - 16], 16");                       // include the uniform allocator header
    emitter.instruction("lea rax, [r9 + 16]");                                 // provide the user pointer expected by shared insertion cleanup
    emitter.instruction("call __rt_heap_free_insert");                         // coalesce this cached block through the ordered free-list path
    abi::emit_symbol_address(emitter, "r8", "_gc_frees");
    emitter.instruction("sub QWORD PTR [r8], 1");                              // avoid recounting a block already freed into the cache
    emitter.instruction("jmp __rt_gc_mem_caches_bin");                         // continue draining the same bin
    emitter.label("__rt_gc_mem_caches_next_bin");
    emitter.instruction("add QWORD PTR [rbp - 24], 8");                        // advance to the next pointer-sized bin slot
    emitter.instruction("jmp __rt_gc_mem_caches_bin");                         // drain the next size class
    emitter.label("__rt_gc_mem_caches_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                      // return bytes removed from specialized caches
    emitter.instruction("leave");                                               // release locals and restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the builtin wrapper
}

/// Emits the AArch64 scan that counts live cycle-collector candidate blocks.
fn emit_gc_roots_aarch64(emitter: &mut Emitter) {
    emitter.label("__rt_gc_status_roots");
    abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    abi::emit_load_symbol_to_reg(emitter, "x10", "_heap_off", 0);
    emitter.instruction("add x10, x9, x10");                                   // compute the current heap scan end
    emitter.instruction("mov x0, #0");                                          // initialize the candidate count
    emitter.label("__rt_gc_status_roots_loop");
    emitter.instruction("cmp x9, x10");                                        // has the heap scan reached its end?
    emitter.instruction("b.hs __rt_gc_status_roots_done");                      // yes, return the live candidate count
    emitter.instruction("ldr w11, [x9]");                                      // load payload size for candidate checks and advancement
    emitter.instruction("ldr w12, [x9, #4]");                                  // load the block's live refcount
    emitter.instruction("cbz w12, __rt_gc_status_roots_next");                  // free blocks are not roots
    emitter.instruction("ldr x13, [x9, #8]");                                  // load heap kind and indexed-array value type
    emitter.instruction("and x14, x13, #0xff");                                // isolate the heap kind
    emitter.instruction("cmp x14, #3");                                        // hashes, objects, and mixed boxes are candidates
    emitter.instruction("b.hs __rt_gc_status_roots_kind_high");                 // inspect the upper candidate range
    emitter.instruction("cmp x14, #2");                                        // indexed arrays need value-type qualification
    emitter.instruction("b.ne __rt_gc_status_roots_next");                     // strings and raw blocks are not candidates
    emitter.instruction("lsr x13, x13, #8");                                   // move the indexed-array value type into low bits
    emitter.instruction("and x13, x13, #0x7f");                                // remove the persistent COW flag
    emitter.instruction("cmp x13, #4");                                        // refcounted element types begin at nested arrays
    emitter.instruction("b.lo __rt_gc_status_roots_next");                     // scalar arrays cannot form cycles
    emitter.instruction("cmp x13, #7");                                        // mixed is the final supported refcounted element type
    emitter.instruction("b.ls __rt_gc_status_roots_count");                    // count this refcounted indexed array
    emitter.instruction("b __rt_gc_status_roots_next");                        // ignore unknown value types
    emitter.label("__rt_gc_status_roots_kind_high");
    emitter.instruction("cmp x14, #5");                                        // kind 5 is the last collector-managed shape
    emitter.instruction("b.hi __rt_gc_status_roots_next");                     // ignore other heap kinds
    emitter.label("__rt_gc_status_roots_count");
    emitter.instruction("add x0, x0, #1");                                     // include this live collector candidate
    emitter.label("__rt_gc_status_roots_next");
    emitter.instruction("add x9, x9, x11");                                    // advance by payload size
    emitter.instruction("add x9, x9, #16");                                    // skip the uniform heap header
    emitter.instruction("b __rt_gc_status_roots_loop");                        // inspect the next heap block
    emitter.label("__rt_gc_status_roots_done");
    emitter.instruction("ret");                                                 // return the current candidate count
}

/// Emits the x86_64 scan that counts live cycle-collector candidate blocks.
fn emit_gc_roots_x86_64(emitter: &mut Emitter) {
    emitter.label("__rt_gc_status_roots");
    abi::emit_symbol_address(emitter, "r8", "_heap_buf");
    abi::emit_load_symbol_to_reg(emitter, "r9", "_heap_off", 0);
    emitter.instruction("add r9, r8");                                          // compute the current heap scan end
    emitter.instruction("xor eax, eax");                                        // initialize the candidate count
    emitter.label("__rt_gc_status_roots_loop");
    emitter.instruction("cmp r8, r9");                                          // has the heap scan reached its end?
    emitter.instruction("jae __rt_gc_status_roots_done");                       // yes, return the live candidate count
    emitter.instruction("mov r10d, DWORD PTR [r8]");                           // load payload size for checks and advancement
    emitter.instruction("mov r11d, DWORD PTR [r8 + 4]");                       // load the block's live refcount
    emitter.instruction("test r11d, r11d");                                     // is this block live?
    emitter.instruction("jz __rt_gc_status_roots_next");                        // free blocks are not roots
    emitter.instruction("mov rdx, QWORD PTR [r8 + 8]");                        // load heap kind and indexed-array value type
    emitter.instruction("mov rcx, rdx");                                        // preserve the packed kind while isolating its low byte
    emitter.instruction("and rcx, 0xff");                                       // isolate the heap kind
    emitter.instruction("cmp rcx, 3");                                          // hashes, objects, and mixed boxes are candidates
    emitter.instruction("jae __rt_gc_status_roots_kind_high");                  // inspect the upper candidate range
    emitter.instruction("cmp rcx, 2");                                          // indexed arrays need value-type qualification
    emitter.instruction("jne __rt_gc_status_roots_next");                       // strings and raw blocks are not candidates
    emitter.instruction("shr rdx, 8");                                          // move the indexed-array value type into low bits
    emitter.instruction("and rdx, 0x7f");                                       // remove the persistent COW flag
    emitter.instruction("cmp rdx, 4");                                          // refcounted element types begin at nested arrays
    emitter.instruction("jb __rt_gc_status_roots_next");                        // scalar arrays cannot form cycles
    emitter.instruction("cmp rdx, 7");                                          // mixed is the final supported refcounted element type
    emitter.instruction("jbe __rt_gc_status_roots_count");                      // count this refcounted indexed array
    emitter.instruction("jmp __rt_gc_status_roots_next");                       // ignore unknown value types
    emitter.label("__rt_gc_status_roots_kind_high");
    emitter.instruction("cmp rcx, 5");                                          // kind 5 is the last collector-managed shape
    emitter.instruction("ja __rt_gc_status_roots_next");                        // ignore other heap kinds
    emitter.label("__rt_gc_status_roots_count");
    emitter.instruction("add rax, 1");                                          // include this live collector candidate
    emitter.label("__rt_gc_status_roots_next");
    emitter.instruction("add r8, r10");                                         // advance by payload size
    emitter.instruction("add r8, 16");                                          // skip the uniform heap header
    emitter.instruction("jmp __rt_gc_status_roots_loop");                       // inspect the next heap block
    emitter.label("__rt_gc_status_roots_done");
    emitter.instruction("ret");                                                 // return the current candidate count
}

/// Emits AArch64 request and collector phase timing helpers.
fn emit_gc_timing_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_gc_request_start");
    emitter.instruction("sub sp, sp, #32");                                     // reserve a standard helper frame
    emitter.instruction("stp x29, x30, [sp, #16]");                            // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #16");                                   // establish the helper frame pointer
    emitter.instruction("bl __rt_microtime");                                  // read the request start time in seconds
    abi::emit_symbol_address(emitter, "x9", "_gc_application_started");
    emitter.instruction("str d0, [x9]");                                       // publish the request start timestamp
    for symbol in [
        "_gc_collector_started",
        "_gc_free_started",
        "_gc_destructor_started",
        "_gc_collector_time",
        "_gc_destructor_time",
        "_gc_free_time",
        "_gc_destructor_depth",
    ] {
        abi::emit_store_zero_to_symbol(emitter, symbol, 0);
    }
    emitter.instruction("ldp x29, x30, [sp, #16]");                            // restore the caller frame and return address
    emitter.instruction("add sp, sp, #32");                                    // release the helper frame
    emitter.instruction("ret");                                                 // return after initializing request timing

    emitter.label_global("__rt_gc_collector_begin");
    emitter.instruction("sub sp, sp, #32");                                     // reserve a standard helper frame
    emitter.instruction("stp x29, x30, [sp, #16]");                            // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #16");                                   // establish the helper frame pointer
    emitter.instruction("bl __rt_microtime");                                  // read the collector phase start time
    abi::emit_symbol_address(emitter, "x9", "_gc_collector_started");
    emitter.instruction("str d0, [x9]");                                       // publish the collector phase timestamp
    emitter.instruction("ldp x29, x30, [sp, #16]");                            // restore the caller frame and return address
    emitter.instruction("add sp, sp, #32");                                    // release the helper frame
    emitter.instruction("ret");                                                 // return to the collector

    emitter.label_global("__rt_gc_free_begin");
    emitter.instruction("sub sp, sp, #32");                                     // reserve a standard helper frame
    emitter.instruction("stp x29, x30, [sp, #16]");                            // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #16");                                   // establish the helper frame pointer
    emitter.instruction("bl __rt_microtime");                                  // read the graph-free phase start time
    abi::emit_symbol_address(emitter, "x9", "_gc_free_started");
    emitter.instruction("str d0, [x9]");                                       // publish the graph-free phase timestamp
    emitter.instruction("ldp x29, x30, [sp, #16]");                            // restore the caller frame and return address
    emitter.instruction("add sp, sp, #32");                                    // release the helper frame
    emitter.instruction("ret");                                                 // return to the collector free pass

    emitter.label_global("__rt_gc_collector_end");
    emitter.instruction("sub sp, sp, #32");                                     // reserve a standard helper frame
    emitter.instruction("stp x29, x30, [sp, #16]");                            // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #16");                                   // establish the helper frame pointer
    emitter.instruction("bl __rt_microtime");                                  // read the common collector and free end time
    abi::emit_symbol_address(emitter, "x9", "_gc_collector_started");
    emitter.instruction("ldr d1, [x9]");                                       // load the collector phase start timestamp
    emitter.instruction("fsub d1, d0, d1");                                    // calculate this collector pass duration
    abi::emit_symbol_address(emitter, "x9", "_gc_collector_time");
    emitter.instruction("ldr d2, [x9]");                                       // load prior cumulative collector seconds
    emitter.instruction("fadd d2, d2, d1");                                    // include this pass duration
    emitter.instruction("str d2, [x9]");                                       // persist cumulative collector seconds
    abi::emit_symbol_address(emitter, "x9", "_gc_free_started");
    emitter.instruction("ldr d1, [x9]");                                       // load the graph-free phase start timestamp
    emitter.instruction("fsub d1, d0, d1");                                    // calculate this graph-free duration
    abi::emit_symbol_address(emitter, "x9", "_gc_free_time");
    emitter.instruction("ldr d2, [x9]");                                       // load prior cumulative graph-free seconds
    emitter.instruction("fadd d2, d2, d1");                                    // include this free-pass duration
    emitter.instruction("str d2, [x9]");                                       // persist cumulative graph-free seconds
    emitter.instruction("ldp x29, x30, [sp, #16]");                            // restore the caller frame and return address
    emitter.instruction("add sp, sp, #32");                                    // release the helper frame
    emitter.instruction("ret");                                                 // return to collector finalization

    emitter.label_global("__rt_gc_destructor_begin");
    emitter.instruction("sub sp, sp, #32");                                     // reserve a standard helper frame for an optional clock read
    emitter.instruction("stp x29, x30, [sp, #16]");                            // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #16");                                   // establish the helper frame pointer
    abi::emit_load_symbol_to_reg(emitter, "x9", "_gc_collecting", 0);
    emitter.instruction("cbz x9, __rt_gc_destructor_begin_done");               // only collector-triggered destructors contribute
    abi::emit_symbol_address(emitter, "x10", "_gc_destructor_depth");
    emitter.instruction("ldr x9, [x10]");                                      // load nested destructor depth
    emitter.instruction("add x11, x9, #1");                                    // enter this destructor frame
    emitter.instruction("str x11, [x10]");                                     // persist nested destructor depth
    emitter.instruction("cbnz x9, __rt_gc_destructor_begin_done");              // nested destructors share the outer timing interval
    emitter.instruction("bl __rt_microtime");                                  // read the outer destructor interval start
    abi::emit_symbol_address(emitter, "x9", "_gc_destructor_started");
    emitter.instruction("str d0, [x9]");                                       // publish the outer destructor start timestamp
    emitter.label("__rt_gc_destructor_begin_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                            // restore the caller frame and return address
    emitter.instruction("add sp, sp, #32");                                    // release the helper frame
    emitter.instruction("ret");                                                 // return to object cleanup

    emitter.label_global("__rt_gc_destructor_end");
    emitter.instruction("sub sp, sp, #32");                                     // reserve a standard helper frame for an optional clock read
    emitter.instruction("stp x29, x30, [sp, #16]");                            // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #16");                                   // establish the helper frame pointer
    abi::emit_load_symbol_to_reg(emitter, "x9", "_gc_collecting", 0);
    emitter.instruction("cbz x9, __rt_gc_destructor_end_done");                 // ordinary destruction is outside collector timing
    abi::emit_symbol_address(emitter, "x10", "_gc_destructor_depth");
    emitter.instruction("ldr x9, [x10]");                                      // load nested destructor depth
    emitter.instruction("cbz x9, __rt_gc_destructor_end_done");                 // tolerate an unmatched end without underflow
    emitter.instruction("sub x9, x9, #1");                                     // leave this destructor frame
    emitter.instruction("str x9, [x10]");                                      // persist nested destructor depth
    emitter.instruction("cbnz x9, __rt_gc_destructor_end_done");                // only the outer interval updates elapsed time
    emitter.instruction("bl __rt_microtime");                                  // read the outer destructor interval end
    abi::emit_symbol_address(emitter, "x9", "_gc_destructor_started");
    emitter.instruction("ldr d1, [x9]");                                       // load the outer destructor start timestamp
    emitter.instruction("fsub d1, d0, d1");                                    // calculate this destructor interval
    abi::emit_symbol_address(emitter, "x9", "_gc_destructor_time");
    emitter.instruction("ldr d2, [x9]");                                       // load prior cumulative destructor seconds
    emitter.instruction("fadd d2, d2, d1");                                    // include this interval
    emitter.instruction("str d2, [x9]");                                       // persist cumulative destructor seconds
    emitter.label("__rt_gc_destructor_end_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                            // restore the caller frame and return address
    emitter.instruction("add sp, sp, #32");                                    // release the helper frame
    emitter.instruction("ret");                                                 // return to object cleanup

    emitter.label("__rt_gc_status_application_time");
    emitter.instruction("sub sp, sp, #32");                                     // reserve a standard helper frame around the clock read
    emitter.instruction("stp x29, x30, [sp, #16]");                            // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #16");                                   // establish the helper frame pointer
    emitter.instruction("bl __rt_microtime");                                  // read the current wall-clock timestamp
    abi::emit_symbol_address(emitter, "x9", "_gc_application_started");
    emitter.instruction("ldr d1, [x9]");                                       // load the request start timestamp
    emitter.instruction("fsub d0, d0, d1");                                    // calculate elapsed request time
    abi::emit_symbol_address(emitter, "x9", "_gc_collector_time");
    emitter.instruction("ldr d1, [x9]");                                       // load cumulative collector time
    emitter.instruction("fsub d0, d0, d1");                                    // report time spent outside the collector
    emitter.instruction("fcmp d0, #0.0");                                      // guard against clock granularity and rounding
    emitter.instruction("b.ge __rt_gc_status_application_nonnegative");         // keep a valid nonnegative duration
    emitter.instruction("fmov d0, xzr");                                       // clamp a negative duration to zero
    emitter.label("__rt_gc_status_application_nonnegative");
    emitter.instruction("fmov x0, d0");                                        // return the IEEE-754 bits through the scalar metric ABI
    emitter.instruction("ldp x29, x30, [sp, #16]");                            // restore the caller frame and return address
    emitter.instruction("add sp, sp, #32");                                    // release the helper frame
    emitter.instruction("ret");                                                 // return the live application-time metric
    emitter.label("__rt_gc_status_collector_time");
    abi::emit_load_symbol_to_reg(emitter, "x0", "_gc_collector_time", 0);
    emitter.instruction("ret");                                                 // return cumulative collector-time bits
    emitter.label("__rt_gc_status_destructor_time");
    abi::emit_load_symbol_to_reg(emitter, "x0", "_gc_destructor_time", 0);
    emitter.instruction("ret");                                                 // return cumulative destructor-time bits
    emitter.label("__rt_gc_status_free_time");
    abi::emit_load_symbol_to_reg(emitter, "x0", "_gc_free_time", 0);
    emitter.instruction("ret");                                                 // return cumulative graph-free-time bits
}

/// Emits x86_64 request and collector phase timing helpers.
fn emit_gc_timing_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_gc_request_start");
    emitter.instruction("push rbp");                                            // align the stack and preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a standard helper frame
    emitter.instruction("call __rt_microtime");                                // read the request start time in seconds
    abi::emit_symbol_address(emitter, "r8", "_gc_application_started");
    emitter.instruction("movsd QWORD PTR [r8], xmm0");                         // publish the request start timestamp
    for symbol in [
        "_gc_collector_started",
        "_gc_free_started",
        "_gc_destructor_started",
        "_gc_collector_time",
        "_gc_destructor_time",
        "_gc_free_time",
        "_gc_destructor_depth",
    ] {
        abi::emit_store_zero_to_symbol(emitter, symbol, 0);
    }
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return after initializing request timing

    emitter.label_global("__rt_gc_collector_begin");
    emitter.instruction("push rbp");                                            // align the stack and preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a standard helper frame
    emitter.instruction("call __rt_microtime");                                // read the collector phase start time
    abi::emit_symbol_address(emitter, "r8", "_gc_collector_started");
    emitter.instruction("movsd QWORD PTR [r8], xmm0");                         // publish the collector phase timestamp
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the collector

    emitter.label_global("__rt_gc_free_begin");
    emitter.instruction("push rbp");                                            // align the stack and preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a standard helper frame
    emitter.instruction("call __rt_microtime");                                // read the graph-free phase start time
    abi::emit_symbol_address(emitter, "r8", "_gc_free_started");
    emitter.instruction("movsd QWORD PTR [r8], xmm0");                         // publish the graph-free phase timestamp
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the collector free pass

    emitter.label_global("__rt_gc_collector_end");
    emitter.instruction("push rbp");                                            // align the stack and preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a standard helper frame
    emitter.instruction("call __rt_microtime");                                // read the common collector and free end time
    abi::emit_symbol_address(emitter, "r8", "_gc_collector_started");
    emitter.instruction("movsd xmm1, QWORD PTR [r8]");                         // load the collector phase start timestamp
    emitter.instruction("subsd xmm0, xmm1");                                   // calculate this collector pass duration
    abi::emit_symbol_address(emitter, "r8", "_gc_collector_time");
    emitter.instruction("movsd xmm2, QWORD PTR [r8]");                         // load prior cumulative collector seconds
    emitter.instruction("addsd xmm2, xmm0");                                   // include this pass duration
    emitter.instruction("movsd QWORD PTR [r8], xmm2");                         // persist cumulative collector seconds
    emitter.instruction("addsd xmm0, xmm1");                                   // recover the common phase end timestamp
    abi::emit_symbol_address(emitter, "r8", "_gc_free_started");
    emitter.instruction("movsd xmm1, QWORD PTR [r8]");                         // load the graph-free phase start timestamp
    emitter.instruction("subsd xmm0, xmm1");                                   // calculate this graph-free duration
    abi::emit_symbol_address(emitter, "r8", "_gc_free_time");
    emitter.instruction("movsd xmm2, QWORD PTR [r8]");                         // load prior cumulative graph-free seconds
    emitter.instruction("addsd xmm2, xmm0");                                   // include this free-pass duration
    emitter.instruction("movsd QWORD PTR [r8], xmm2");                         // persist cumulative graph-free seconds
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to collector finalization

    emitter.label_global("__rt_gc_destructor_begin");
    emitter.instruction("push rbp");                                            // align the stack and preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a standard helper frame
    abi::emit_load_symbol_to_reg(emitter, "r8", "_gc_collecting", 0);
    emitter.instruction("test r8, r8");                                         // is this destruction collector-triggered?
    emitter.instruction("jz __rt_gc_destructor_begin_done");                    // ordinary destruction does not contribute
    abi::emit_symbol_address(emitter, "r9", "_gc_destructor_depth");
    emitter.instruction("mov r8, QWORD PTR [r9]");                             // load nested destructor depth
    emitter.instruction("lea r10, [r8 + 1]");                                  // enter this destructor frame
    emitter.instruction("mov QWORD PTR [r9], r10");                            // persist nested destructor depth
    emitter.instruction("test r8, r8");                                         // is an outer destructor already timed?
    emitter.instruction("jnz __rt_gc_destructor_begin_done");                   // nested destructors share the outer interval
    emitter.instruction("call __rt_microtime");                                // read the outer destructor interval start
    abi::emit_symbol_address(emitter, "r8", "_gc_destructor_started");
    emitter.instruction("movsd QWORD PTR [r8], xmm0");                         // publish the outer destructor start timestamp
    emitter.label("__rt_gc_destructor_begin_done");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to object cleanup

    emitter.label_global("__rt_gc_destructor_end");
    emitter.instruction("push rbp");                                            // align the stack and preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a standard helper frame
    abi::emit_load_symbol_to_reg(emitter, "r8", "_gc_collecting", 0);
    emitter.instruction("test r8, r8");                                         // is this destruction collector-triggered?
    emitter.instruction("jz __rt_gc_destructor_end_done");                      // ordinary destruction is outside collector timing
    abi::emit_symbol_address(emitter, "r9", "_gc_destructor_depth");
    emitter.instruction("mov r8, QWORD PTR [r9]");                             // load nested destructor depth
    emitter.instruction("test r8, r8");                                         // was a matching begin observed?
    emitter.instruction("jz __rt_gc_destructor_end_done");                      // tolerate an unmatched end without underflow
    emitter.instruction("sub r8, 1");                                           // leave this destructor frame
    emitter.instruction("mov QWORD PTR [r9], r8");                             // persist nested destructor depth
    emitter.instruction("test r8, r8");                                         // did the outer interval finish?
    emitter.instruction("jnz __rt_gc_destructor_end_done");                     // nested completion does not update elapsed time
    emitter.instruction("call __rt_microtime");                                // read the outer destructor interval end
    abi::emit_symbol_address(emitter, "r8", "_gc_destructor_started");
    emitter.instruction("subsd xmm0, QWORD PTR [r8]");                         // calculate this destructor interval
    abi::emit_symbol_address(emitter, "r8", "_gc_destructor_time");
    emitter.instruction("addsd xmm0, QWORD PTR [r8]");                         // include prior cumulative destructor seconds
    emitter.instruction("movsd QWORD PTR [r8], xmm0");                         // persist cumulative destructor seconds
    emitter.label("__rt_gc_destructor_end_done");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to object cleanup

    emitter.label("__rt_gc_status_application_time");
    emitter.instruction("push rbp");                                            // align the stack and preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a standard helper frame
    emitter.instruction("call __rt_microtime");                                // read the current wall-clock timestamp
    abi::emit_symbol_address(emitter, "r8", "_gc_application_started");
    emitter.instruction("subsd xmm0, QWORD PTR [r8]");                         // calculate elapsed request time
    abi::emit_symbol_address(emitter, "r8", "_gc_collector_time");
    emitter.instruction("subsd xmm0, QWORD PTR [r8]");                         // report time spent outside the collector
    emitter.instruction("xorpd xmm1, xmm1");                                   // materialize zero for the nonnegative clamp
    emitter.instruction("comisd xmm0, xmm1");                                  // guard against clock granularity and rounding
    emitter.instruction("jae __rt_gc_status_application_nonnegative");          // keep a valid nonnegative duration
    emitter.instruction("xorpd xmm0, xmm0");                                   // clamp a negative duration to zero
    emitter.label("__rt_gc_status_application_nonnegative");
    emitter.instruction("movq rax, xmm0");                                     // return the IEEE-754 bits through the scalar metric ABI
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the live application-time metric
    emitter.label("__rt_gc_status_collector_time");
    abi::emit_load_symbol_to_reg(emitter, "rax", "_gc_collector_time", 0);
    emitter.instruction("ret");                                                 // return cumulative collector-time bits
    emitter.label("__rt_gc_status_destructor_time");
    abi::emit_load_symbol_to_reg(emitter, "rax", "_gc_destructor_time", 0);
    emitter.instruction("ret");                                                 // return cumulative destructor-time bits
    emitter.label("__rt_gc_status_free_time");
    abi::emit_load_symbol_to_reg(emitter, "rax", "_gc_free_time", 0);
    emitter.instruction("ret");                                                 // return cumulative graph-free-time bits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{AppleVariant, Platform, Target};

    /// Verifies every supported target emits cache, root, and timing runtime paths.
    #[test]
    fn gc_control_emits_live_metrics_for_all_supported_targets() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new_apple(Arch::AArch64, AppleVariant::IOS),
            Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_gc_control(&mut emitter);
            let asm = emitter.output();
            for symbol in [
                "__rt_gc_mem_caches:",
                "__rt_gc_status_roots:",
                "__rt_gc_request_start:",
                "__rt_gc_collector_begin:",
                "__rt_gc_free_begin:",
                "__rt_gc_collector_end:",
                "__rt_gc_destructor_begin:",
                "__rt_gc_destructor_end:",
                "__rt_gc_status_application_time:",
            ] {
                assert!(asm.contains(symbol), "missing {symbol} on {target:?}\n{asm}");
            }
            match target.arch {
                Arch::AArch64 => {
                    assert!(asm.contains("bl __rt_heap_free_insert"));
                    assert!(asm.contains("fmov x0, d0"));
                }
                Arch::X86_64 => {
                    assert!(asm.contains("call __rt_heap_free_insert"));
                    assert!(asm.contains("movq rax, xmm0"));
                }
            }
        }
    }
}
