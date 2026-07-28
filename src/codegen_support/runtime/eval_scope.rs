//! Purpose:
//! Emits core runtime helpers for scope-only literal eval AOT fragments.
//! Provides a small materialized name-to-Mixed-cell map without linking the
//! optional Rust eval interpreter bridge.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` when `eval_scope` is
//!   needed without `eval_bridge`.
//!
//! Key details:
//! - Entry names are generated static data labels, so the scope stores borrowed
//!   name pointers instead of copying name bytes.
//! - Owned Mixed cells are released on overwrite and scope free.

use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

const EVAL_STATUS_OK: i64 = 0;
const EVAL_STATUS_RUNTIME_FATAL: i64 = 2;
const EVAL_SCOPE_FLAG_PRESENT: i64 = 1;
const EVAL_SCOPE_FLAG_DIRTY: i64 = 1 << 2;
const EVAL_SCOPE_FLAG_OWNED: i64 = 1 << 4;

/// Emits the target-specific eval-scope core helper surface.
pub(crate) fn emit_eval_scope_runtime(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: eval scope core helpers ---");
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64_eval_scope_runtime(emitter),
        Arch::X86_64 => emit_x86_64_eval_scope_runtime(emitter),
    }
}

/// Emits ARM64 eval-scope helpers. `__elephc_eval_value_null` comes from the
/// eval bridge value wrappers, which scope-only programs also emit.
fn emit_aarch64_eval_scope_runtime(emitter: &mut Emitter) {
    emit_aarch64_eval_scope_new(emitter);
    emit_aarch64_eval_scope_free(emitter);
    emit_aarch64_eval_scope_set(emitter);
    emit_aarch64_eval_scope_get(emitter);
}

/// Emits the ARM64 scope allocator.
fn emit_aarch64_eval_scope_new(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_scope_new");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve the caller frame while allocating the scope header
    emitter.instruction("mov x29, sp");                                         // establish a stable frame for the nested allocator call
    emitter.instruction("mov x0, #8");                                          // scope header stores one head pointer
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the scope header from the core heap
    emitter.instruction("str xzr, [x0]");                                       // initialize the entry-list head to null
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the caller frame after allocation
    emitter.instruction("ret");                                                 // return the scope handle in x0
}

/// Emits the ARM64 scope destructor.
fn emit_aarch64_eval_scope_free(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_scope_free");
    emitter.instruction("cbz x0, __elephc_eval_scope_free_done");               // null scope handles are already free
    emitter.instruction("sub sp, sp, #48");                                     // reserve a frame for callee-saved scan state
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address across release calls
    emitter.instruction("stp x19, x20, [sp, #16]");                             // preserve scope and current-entry registers
    emitter.instruction("str x21, [sp, #32]");                                  // preserve the next-entry register
    emitter.instruction("mov x29, sp");                                         // establish a stable frame for nested runtime calls
    emitter.instruction("mov x19, x0");                                         // keep the scope header pointer across entry releases
    emitter.instruction("ldr x20, [x19]");                                      // load the first entry in the scope list
    emitter.label("__elephc_eval_scope_free_loop");
    emitter.instruction("cbz x20, __elephc_eval_scope_free_header");            // stop when every entry has been released
    emitter.instruction("ldr x21, [x20]");                                      // save entry->next before freeing the current entry
    emitter.instruction("ldr x9, [x20, #32]");                                  // load the entry ABI flags
    emitter.instruction(&format!("tst x9, #{}", EVAL_SCOPE_FLAG_OWNED));        // check whether the scope owns this Mixed cell
    emitter.instruction("b.eq __elephc_eval_scope_free_entry");                 // borrowed cells are not released by the scope
    emitter.instruction("ldr x0, [x20, #24]");                                  // load the owned Mixed cell pointer
    emitter.instruction("cbz x0, __elephc_eval_scope_free_entry");              // tolerate null owned cells defensively
    emitter.instruction("bl __rt_decref_mixed");                                // release the scope-owned Mixed cell
    emitter.label("__elephc_eval_scope_free_entry");
    emitter.instruction("mov x0, x20");                                         // pass the current entry allocation to the heap free helper
    emitter.instruction("bl __rt_heap_free");                                   // release the entry record itself
    emitter.instruction("mov x20, x21");                                        // advance to the saved next entry
    emitter.instruction("b __elephc_eval_scope_free_loop");                     // continue freeing entries
    emitter.label("__elephc_eval_scope_free_header");
    emitter.instruction("mov x0, x19");                                         // pass the scope header allocation to the heap free helper
    emitter.instruction("bl __rt_heap_free");                                   // release the scope header after all entries
    emitter.instruction("ldr x21, [sp, #32]");                                  // restore the saved next-entry register
    emitter.instruction("ldp x19, x20, [sp, #16]");                             // restore callee-saved scan registers
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the scope-free frame
    emitter.label("__elephc_eval_scope_free_done");
    emitter.instruction("ret");                                                 // return after the scope is fully released
}

/// Emits the ARM64 scope setter.
fn emit_aarch64_eval_scope_set(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_scope_set");
    emitter.instruction("cbz x0, __elephc_eval_scope_set_fatal");               // reject null scope handles
    emitter.instruction("cbz x2, __elephc_eval_scope_set_frame");               // empty names do not need a readable pointer
    emitter.instruction("cbz x1, __elephc_eval_scope_set_fatal");               // non-empty names must provide bytes
    emitter.label("__elephc_eval_scope_set_frame");
    emitter.instruction("sub sp, sp, #96");                                     // reserve a frame for inputs and callee-saved scan state
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address across runtime calls
    emitter.instruction("stp x19, x20, [sp, #16]");                             // preserve scope and name pointer
    emitter.instruction("stp x21, x22, [sp, #32]");                             // preserve name length and cell pointer
    emitter.instruction("stp x23, x24, [sp, #48]");                             // preserve flags and previous-next pointer
    emitter.instruction("stp x25, x26, [sp, #64]");                             // preserve current entry and allocated entry pointer
    emitter.instruction("mov x29, sp");                                         // establish a stable frame for nested runtime calls
    emitter.instruction("mov x19, x0");                                         // save scope header pointer
    emitter.instruction("mov x20, x1");                                         // save name byte pointer
    emitter.instruction("mov x21, x2");                                         // save name byte length
    emitter.instruction("mov x22, x3");                                         // save Mixed cell pointer
    emitter.instruction("mov x23, x4");                                         // save caller-provided ABI flags
    emitter.instruction("mov x24, x19");                                        // previous-next pointer initially addresses scope->head
    emitter.instruction("ldr x25, [x24]");                                      // load the first candidate entry
    emitter.label("__elephc_eval_scope_set_probe");
    emitter.instruction("cbz x25, __elephc_eval_scope_set_insert");             // insert when the name is absent
    emitter.instruction("ldr x9, [x25, #16]");                                  // load candidate name length
    emitter.instruction("cmp x9, x21");                                         // compare candidate length with requested length
    emitter.instruction("b.ne __elephc_eval_scope_set_next");                   // different lengths cannot match
    emitter.instruction("ldr x10, [x25, #8]");                                  // load candidate name bytes
    emitter.instruction("mov x11, #0");                                         // byte index for the equality loop
    emitter.label("__elephc_eval_scope_set_cmp");
    emitter.instruction("cmp x11, x21");                                        // have all bytes matched?
    emitter.instruction("b.eq __elephc_eval_scope_set_update");                 // equal length and bytes select this entry
    emitter.instruction("ldrb w12, [x10, x11]");                                // load one existing name byte
    emitter.instruction("ldrb w13, [x20, x11]");                                // load the corresponding requested name byte
    emitter.instruction("cmp w12, w13");                                        // compare candidate and requested name bytes
    emitter.instruction("b.ne __elephc_eval_scope_set_next");                   // any byte mismatch means this entry is not the target
    emitter.instruction("add x11, x11, #1");                                    // advance to the next byte
    emitter.instruction("b __elephc_eval_scope_set_cmp");                       // continue comparing this candidate name
    emitter.label("__elephc_eval_scope_set_next");
    emitter.instruction("mov x24, x25");                                        // previous-next pointer now addresses current->next
    emitter.instruction("ldr x25, [x25]");                                      // advance to the next entry
    emitter.instruction("b __elephc_eval_scope_set_probe");                     // continue scanning the entry list
    emitter.label("__elephc_eval_scope_set_update");
    emitter.instruction("ldr x9, [x25, #32]");                                  // load old entry flags
    emitter.instruction(&format!("tst x9, #{}", EVAL_SCOPE_FLAG_OWNED));        // check whether the old cell is scope-owned
    emitter.instruction("b.eq __elephc_eval_scope_set_store");                  // borrowed old cells do not need release
    emitter.instruction("ldr x0, [x25, #24]");                                  // load the old Mixed cell pointer
    emitter.instruction("cmp x0, x22");                                         // compare old and replacement cells
    emitter.instruction("b.eq __elephc_eval_scope_set_store");                  // retaining the same cell must not decref it
    emitter.instruction("cbz x0, __elephc_eval_scope_set_store");               // tolerate null old cells defensively
    emitter.instruction("bl __rt_decref_mixed");                                // release the overwritten owned Mixed cell
    emitter.label("__elephc_eval_scope_set_store");
    emitter.instruction("str x22, [x25, #24]");                                 // store the replacement Mixed cell
    emitter.instruction(&format!("mov x9, #{}", EVAL_SCOPE_FLAG_PRESENT | EVAL_SCOPE_FLAG_DIRTY)); // materialize visible and dirty ABI bits
    emitter.instruction("orr x9, x23, x9");                                     // merge caller ownership flags with core visibility flags
    emitter.instruction("str x9, [x25, #32]");                                  // publish the updated ABI flags
    emitter.instruction(&format!("mov x0, #{}", EVAL_STATUS_OK));               // report successful scope write
    emitter.instruction("b __elephc_eval_scope_set_done");                      // restore the frame and return
    emitter.label("__elephc_eval_scope_set_insert");
    emitter.instruction("mov x0, #40");                                         // entry records store next, name, length, cell, and flags
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate a new entry record
    emitter.instruction("mov x26, x0");                                         // keep the new entry pointer while initializing fields
    emitter.instruction("str x25, [x26]");                                      // new entry next points at the old successor, normally null
    emitter.instruction("str x20, [x26, #8]");                                  // store the borrowed static name pointer
    emitter.instruction("str x21, [x26, #16]");                                 // store the name byte length
    emitter.instruction("str x22, [x26, #24]");                                 // store the Mixed cell pointer
    emitter.instruction(&format!("mov x9, #{}", EVAL_SCOPE_FLAG_PRESENT | EVAL_SCOPE_FLAG_DIRTY)); // materialize visible and dirty ABI bits
    emitter.instruction("orr x9, x23, x9");                                     // merge caller ownership flags with core visibility flags
    emitter.instruction("str x9, [x26, #32]");                                  // store the new entry flags
    emitter.instruction("str x26, [x24]");                                      // link the new entry through the previous-next pointer
    emitter.instruction(&format!("mov x0, #{}", EVAL_STATUS_OK));               // report successful scope write
    emitter.label("__elephc_eval_scope_set_done");
    emitter.instruction("ldp x25, x26, [sp, #64]");                             // restore callee-saved entry registers
    emitter.instruction("ldp x23, x24, [sp, #48]");                             // restore callee-saved flag and link registers
    emitter.instruction("ldp x21, x22, [sp, #32]");                             // restore callee-saved name length and cell registers
    emitter.instruction("ldp x19, x20, [sp, #16]");                             // restore callee-saved scope and name registers
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the setter frame
    emitter.instruction("ret");                                                 // return the eval status in x0
    emitter.label("__elephc_eval_scope_set_fatal");
    emitter.instruction(&format!("mov x0, #{}", EVAL_STATUS_RUNTIME_FATAL));    // report invalid scope/name inputs
    emitter.instruction("ret");                                                 // return the fatal eval status without mutating scope
}

/// Emits the ARM64 scope getter.
fn emit_aarch64_eval_scope_get(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_scope_get");
    emitter.instruction("cbz x0, __elephc_eval_scope_get_fatal");               // reject null scope handles
    emitter.instruction("cbz x2, __elephc_eval_scope_get_search");              // empty names do not need a readable pointer
    emitter.instruction("cbz x1, __elephc_eval_scope_get_fatal");               // non-empty names must provide bytes
    emitter.label("__elephc_eval_scope_get_search");
    emitter.instruction("ldr x9, [x0]");                                        // load the first entry from scope->head
    emitter.label("__elephc_eval_scope_get_probe");
    emitter.instruction("cbz x9, __elephc_eval_scope_get_missing");             // missing names produce null cell and zero flags
    emitter.instruction("ldr x10, [x9, #16]");                                  // load candidate name length
    emitter.instruction("cmp x10, x2");                                         // compare candidate length with requested length
    emitter.instruction("b.ne __elephc_eval_scope_get_next");                   // different lengths cannot match
    emitter.instruction("ldr x10, [x9, #8]");                                   // load candidate name bytes
    emitter.instruction("mov x11, #0");                                         // byte index for the equality loop
    emitter.label("__elephc_eval_scope_get_cmp");
    emitter.instruction("cmp x11, x2");                                         // have all bytes matched?
    emitter.instruction("b.eq __elephc_eval_scope_get_found");                  // equal length and bytes select this entry
    emitter.instruction("ldrb w12, [x10, x11]");                                // load one existing name byte
    emitter.instruction("ldrb w13, [x1, x11]");                                 // load the corresponding requested name byte
    emitter.instruction("cmp w12, w13");                                        // compare candidate and requested name bytes
    emitter.instruction("b.ne __elephc_eval_scope_get_next");                   // any byte mismatch means this entry is not the target
    emitter.instruction("add x11, x11, #1");                                    // advance to the next byte
    emitter.instruction("b __elephc_eval_scope_get_cmp");                       // continue comparing this candidate name
    emitter.label("__elephc_eval_scope_get_next");
    emitter.instruction("ldr x9, [x9]");                                        // advance to the next entry
    emitter.instruction("b __elephc_eval_scope_get_probe");                     // continue scanning the scope list
    emitter.label("__elephc_eval_scope_get_found");
    emitter.instruction("cbz x3, __elephc_eval_scope_get_found_flags");         // skip cell output when caller passed null
    emitter.instruction("ldr x10, [x9, #24]");                                  // load the visible Mixed cell pointer
    emitter.instruction("str x10, [x3]");                                       // write the output Mixed cell pointer
    emitter.label("__elephc_eval_scope_get_found_flags");
    emitter.instruction("cbz x4, __elephc_eval_scope_get_ok");                  // skip flags output when caller passed null
    emitter.instruction("ldr w10, [x9, #32]");                                  // load the low ABI flag bits
    emitter.instruction("str w10, [x4]");                                       // write the output ABI flags
    emitter.instruction("b __elephc_eval_scope_get_ok");                        // finish with success
    emitter.label("__elephc_eval_scope_get_missing");
    emitter.instruction("cbz x3, __elephc_eval_scope_get_missing_flags");       // skip cell output when caller passed null
    emitter.instruction("str xzr, [x3]");                                       // missing variables have no cell pointer
    emitter.label("__elephc_eval_scope_get_missing_flags");
    emitter.instruction("cbz x4, __elephc_eval_scope_get_ok");                  // skip flags output when caller passed null
    emitter.instruction("str wzr, [x4]");                                       // missing variables have zero ABI flags
    emitter.label("__elephc_eval_scope_get_ok");
    emitter.instruction(&format!("mov x0, #{}", EVAL_STATUS_OK));               // report successful scope lookup
    emitter.instruction("ret");                                                 // return the eval status in x0
    emitter.label("__elephc_eval_scope_get_fatal");
    emitter.instruction(&format!("mov x0, #{}", EVAL_STATUS_RUNTIME_FATAL));    // report invalid scope/name inputs
    emitter.instruction("ret");                                                 // return the fatal eval status
}

/// Emits x86_64 eval-scope helpers. `__elephc_eval_value_null` comes from the
/// eval bridge value wrappers, which scope-only programs also emit.
fn emit_x86_64_eval_scope_runtime(emitter: &mut Emitter) {
    emit_x86_64_eval_scope_new(emitter);
    emit_x86_64_eval_scope_free(emitter);
    emit_x86_64_eval_scope_set(emitter);
    emit_x86_64_eval_scope_get(emitter);
}

/// Emits the x86_64 scope allocator.
fn emit_x86_64_eval_scope_new(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_scope_new");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before allocating the scope header
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame for the nested allocator call
    emitter.instruction("mov rax, 8");                                          // scope header stores one head pointer
    emitter.instruction("call __rt_heap_alloc");                                // allocate the scope header from the core heap
    emitter.instruction("mov QWORD PTR [rax], 0");                              // initialize the entry-list head to null
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after allocation
    emitter.instruction("ret");                                                 // return the scope handle in rax
}

/// Emits the x86_64 scope destructor.
fn emit_x86_64_eval_scope_free(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_scope_free");
    emitter.instruction("test rdi, rdi");                                       // null scope handles are already free
    emitter.instruction("jz __elephc_eval_scope_free_done");                    // skip all cleanup for null handles
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before release work
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame for local scan state
    emitter.instruction("sub rsp, 32");                                         // reserve scope, current-entry, and next-entry slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the scope header pointer
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the first entry in the scope list
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // store the current entry pointer
    emitter.label("__elephc_eval_scope_free_loop");
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the current entry pointer
    emitter.instruction("test r10, r10");                                       // have all entries been released?
    emitter.instruction("jz __elephc_eval_scope_free_header");                  // stop when the entry list is exhausted
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // save entry->next before freeing the current entry
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // keep the next entry across release calls
    emitter.instruction("mov r11, QWORD PTR [r10 + 32]");                       // load the entry ABI flags
    emitter.instruction(&format!("test r11, {}", EVAL_SCOPE_FLAG_OWNED));       // check whether the scope owns this Mixed cell
    emitter.instruction("jz __elephc_eval_scope_free_entry");                   // borrowed cells are not released by the scope
    emitter.instruction("mov rax, QWORD PTR [r10 + 24]");                       // load the owned Mixed cell pointer
    emitter.instruction("test rax, rax");                                       // tolerate null owned cells defensively
    emitter.instruction("jz __elephc_eval_scope_free_entry");                   // skip release when the owned cell is null
    emitter.instruction("call __rt_decref_mixed");                              // release the scope-owned Mixed cell
    emitter.label("__elephc_eval_scope_free_entry");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // pass the current entry allocation to the heap free helper
    emitter.instruction("call __rt_heap_free");                                 // release the entry record itself
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the saved next entry
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // make the saved next entry current
    emitter.instruction("jmp __elephc_eval_scope_free_loop");                   // continue freeing entries
    emitter.label("__elephc_eval_scope_free_header");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // pass the scope header allocation to the heap free helper
    emitter.instruction("call __rt_heap_free");                                 // release the scope header after all entries
    emitter.instruction("add rsp, 32");                                         // release local scan slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.label("__elephc_eval_scope_free_done");
    emitter.instruction("ret");                                                 // return after the scope is fully released
}

/// Emits the x86_64 scope setter.
fn emit_x86_64_eval_scope_set(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_scope_set");
    emitter.instruction("test rdi, rdi");                                       // reject null scope handles
    emitter.instruction("jz __elephc_eval_scope_set_fatal");                    // return a runtime-fatal status for null scope
    emitter.instruction("test rdx, rdx");                                       // empty names do not need a readable pointer
    emitter.instruction("jz __elephc_eval_scope_set_frame");                    // skip the pointer check for empty names
    emitter.instruction("test rsi, rsi");                                       // non-empty names must provide bytes
    emitter.instruction("jz __elephc_eval_scope_set_fatal");                    // return a runtime-fatal status for invalid names
    emitter.label("__elephc_eval_scope_set_frame");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before scope mutation
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame for saved inputs and scan state
    emitter.instruction("sub rsp, 64");                                         // reserve scope, name, length, cell, flags, previous-next, current, and index slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save scope header pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save name byte pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save name byte length
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save Mixed cell pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // save caller-provided ABI flags
    emitter.instruction("mov QWORD PTR [rbp - 48], rdi");                       // previous-next pointer initially addresses scope->head
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the first candidate entry
    emitter.instruction("mov QWORD PTR [rbp - 56], r10");                       // save the current candidate entry
    emitter.label("__elephc_eval_scope_set_probe");
    emitter.instruction("mov r9, QWORD PTR [rbp - 56]");                        // reload the current candidate entry
    emitter.instruction("test r9, r9");                                         // is the requested name absent?
    emitter.instruction("jz __elephc_eval_scope_set_insert");                   // insert when the scan reaches the end
    emitter.instruction("mov r10, QWORD PTR [r9 + 16]");                        // load candidate name length
    emitter.instruction("cmp r10, QWORD PTR [rbp - 24]");                       // compare candidate length with requested length
    emitter.instruction("jne __elephc_eval_scope_set_next");                    // different lengths cannot match
    emitter.instruction("mov r10, QWORD PTR [r9 + 8]");                         // load candidate name bytes
    emitter.instruction("xor r11, r11");                                        // byte index for the equality loop
    emitter.label("__elephc_eval_scope_set_cmp");
    emitter.instruction("cmp r11, QWORD PTR [rbp - 24]");                       // have all bytes matched?
    emitter.instruction("je __elephc_eval_scope_set_update");                   // equal length and bytes select this entry
    emitter.instruction("mov al, BYTE PTR [r10 + r11]");                        // load one existing name byte
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload requested name pointer
    emitter.instruction("cmp al, BYTE PTR [r8 + r11]");                         // compare candidate and requested name bytes
    emitter.instruction("jne __elephc_eval_scope_set_next");                    // any byte mismatch means this entry is not the target
    emitter.instruction("add r11, 1");                                          // advance to the next byte
    emitter.instruction("jmp __elephc_eval_scope_set_cmp");                     // continue comparing this candidate name
    emitter.label("__elephc_eval_scope_set_next");
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // previous-next pointer now addresses current->next
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // advance to the next entry
    emitter.instruction("mov QWORD PTR [rbp - 56], r10");                       // save the next candidate as current
    emitter.instruction("jmp __elephc_eval_scope_set_probe");                   // continue scanning the entry list
    emitter.label("__elephc_eval_scope_set_update");
    emitter.instruction("mov r10, QWORD PTR [r9 + 32]");                        // load old entry flags
    emitter.instruction(&format!("test r10, {}", EVAL_SCOPE_FLAG_OWNED));       // check whether the old cell is scope-owned
    emitter.instruction("jz __elephc_eval_scope_set_store");                    // borrowed old cells do not need release
    emitter.instruction("mov rax, QWORD PTR [r9 + 24]");                        // load the old Mixed cell pointer
    emitter.instruction("cmp rax, QWORD PTR [rbp - 32]");                       // compare old and replacement cells
    emitter.instruction("je __elephc_eval_scope_set_store");                    // retaining the same cell must not decref it
    emitter.instruction("test rax, rax");                                       // tolerate null old cells defensively
    emitter.instruction("jz __elephc_eval_scope_set_store");                    // skip release when the old cell is null
    emitter.instruction("call __rt_decref_mixed");                              // release the overwritten owned Mixed cell
    emitter.instruction("mov r9, QWORD PTR [rbp - 56]");                        // reload current entry after the runtime call
    emitter.label("__elephc_eval_scope_set_store");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the replacement Mixed cell
    emitter.instruction("mov QWORD PTR [r9 + 24], r10");                        // store the replacement Mixed cell
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload caller-provided ABI flags
    emitter.instruction(&format!("or r10, {}", EVAL_SCOPE_FLAG_PRESENT | EVAL_SCOPE_FLAG_DIRTY)); // mark the entry visible and dirty
    emitter.instruction("mov QWORD PTR [r9 + 32], r10");                        // publish the updated ABI flags
    emitter.instruction(&format!("mov eax, {}", EVAL_STATUS_OK));               // report successful scope write
    emitter.instruction("jmp __elephc_eval_scope_set_done");                    // restore the frame and return
    emitter.label("__elephc_eval_scope_set_insert");
    emitter.instruction("mov rax, 40");                                         // entry records store next, name, length, cell, and flags
    emitter.instruction("call __rt_heap_alloc");                                // allocate a new entry record
    emitter.instruction("mov r9, QWORD PTR [rbp - 56]");                        // reload old successor, normally null
    emitter.instruction("mov QWORD PTR [rax], r9");                             // new entry next points at the old successor
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload borrowed static name pointer
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store the borrowed static name pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload name byte length
    emitter.instruction("mov QWORD PTR [rax + 16], r10");                       // store the name byte length
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload Mixed cell pointer
    emitter.instruction("mov QWORD PTR [rax + 24], r10");                       // store the Mixed cell pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload caller-provided ABI flags
    emitter.instruction(&format!("or r10, {}", EVAL_SCOPE_FLAG_PRESENT | EVAL_SCOPE_FLAG_DIRTY)); // mark the new entry visible and dirty
    emitter.instruction("mov QWORD PTR [rax + 32], r10");                       // store the new entry flags
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // reload the previous-next pointer
    emitter.instruction("mov QWORD PTR [r11], rax");                            // link the new entry through the previous-next pointer
    emitter.instruction(&format!("mov eax, {}", EVAL_STATUS_OK));               // report successful scope write
    emitter.label("__elephc_eval_scope_set_done");
    emitter.instruction("add rsp, 64");                                         // release saved input and scan slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the eval status in rax
    emitter.label("__elephc_eval_scope_set_fatal");
    emitter.instruction(&format!("mov eax, {}", EVAL_STATUS_RUNTIME_FATAL));    // report invalid scope/name inputs
    emitter.instruction("ret");                                                 // return the fatal eval status without mutating scope
}

/// Emits the x86_64 scope getter.
fn emit_x86_64_eval_scope_get(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_scope_get");
    emitter.instruction("test rdi, rdi");                                       // reject null scope handles
    emitter.instruction("jz __elephc_eval_scope_get_fatal");                    // return a runtime-fatal status for null scope
    emitter.instruction("test rdx, rdx");                                       // empty names do not need a readable pointer
    emitter.instruction("jz __elephc_eval_scope_get_search");                   // skip the pointer check for empty names
    emitter.instruction("test rsi, rsi");                                       // non-empty names must provide bytes
    emitter.instruction("jz __elephc_eval_scope_get_fatal");                    // return a runtime-fatal status for invalid names
    emitter.label("__elephc_eval_scope_get_search");
    emitter.instruction("mov r9, QWORD PTR [rdi]");                             // load the first entry from scope->head
    emitter.label("__elephc_eval_scope_get_probe");
    emitter.instruction("test r9, r9");                                         // has the scan reached the end of the entry list?
    emitter.instruction("jz __elephc_eval_scope_get_missing");                  // missing names produce null cell and zero flags
    emitter.instruction("mov r10, QWORD PTR [r9 + 16]");                        // load candidate name length
    emitter.instruction("cmp r10, rdx");                                        // compare candidate length with requested length
    emitter.instruction("jne __elephc_eval_scope_get_next");                    // different lengths cannot match
    emitter.instruction("mov r10, QWORD PTR [r9 + 8]");                         // load candidate name bytes
    emitter.instruction("xor r11, r11");                                        // byte index for the equality loop
    emitter.label("__elephc_eval_scope_get_cmp");
    emitter.instruction("cmp r11, rdx");                                        // have all bytes matched?
    emitter.instruction("je __elephc_eval_scope_get_found");                    // equal length and bytes select this entry
    emitter.instruction("mov al, BYTE PTR [r10 + r11]");                        // load one existing name byte
    emitter.instruction("cmp al, BYTE PTR [rsi + r11]");                        // compare candidate and requested name bytes
    emitter.instruction("jne __elephc_eval_scope_get_next");                    // any byte mismatch means this entry is not the target
    emitter.instruction("add r11, 1");                                          // advance to the next byte
    emitter.instruction("jmp __elephc_eval_scope_get_cmp");                     // continue comparing this candidate name
    emitter.label("__elephc_eval_scope_get_next");
    emitter.instruction("mov r9, QWORD PTR [r9]");                              // advance to the next entry
    emitter.instruction("jmp __elephc_eval_scope_get_probe");                   // continue scanning the scope list
    emitter.label("__elephc_eval_scope_get_found");
    emitter.instruction("test rcx, rcx");                                       // did the caller request cell output?
    emitter.instruction("jz __elephc_eval_scope_get_found_flags");              // skip cell output for null out_cell
    emitter.instruction("mov r10, QWORD PTR [r9 + 24]");                        // load the visible Mixed cell pointer
    emitter.instruction("mov QWORD PTR [rcx], r10");                            // write the output Mixed cell pointer
    emitter.label("__elephc_eval_scope_get_found_flags");
    emitter.instruction("test r8, r8");                                         // did the caller request flag output?
    emitter.instruction("jz __elephc_eval_scope_get_ok");                       // skip flags output for null out_flags
    emitter.instruction("mov r10d, DWORD PTR [r9 + 32]");                       // load the low ABI flag bits
    emitter.instruction("mov DWORD PTR [r8], r10d");                            // write the output ABI flags
    emitter.instruction("jmp __elephc_eval_scope_get_ok");                      // finish with success
    emitter.label("__elephc_eval_scope_get_missing");
    emitter.instruction("test rcx, rcx");                                       // did the caller request cell output?
    emitter.instruction("jz __elephc_eval_scope_get_missing_flags");            // skip cell output for null out_cell
    emitter.instruction("mov QWORD PTR [rcx], 0");                              // missing variables have no cell pointer
    emitter.label("__elephc_eval_scope_get_missing_flags");
    emitter.instruction("test r8, r8");                                         // did the caller request flag output?
    emitter.instruction("jz __elephc_eval_scope_get_ok");                       // skip flags output for null out_flags
    emitter.instruction("mov DWORD PTR [r8], 0");                               // missing variables have zero ABI flags
    emitter.label("__elephc_eval_scope_get_ok");
    emitter.instruction(&format!("mov eax, {}", EVAL_STATUS_OK));               // report successful scope lookup
    emitter.instruction("ret");                                                 // return the eval status in rax
    emitter.label("__elephc_eval_scope_get_fatal");
    emitter.instruction(&format!("mov eax, {}", EVAL_STATUS_RUNTIME_FATAL));    // report invalid scope/name inputs
    emitter.instruction("ret");                                                 // return the fatal eval status
}

/// Emits a global label with platform C-symbol mangling.
fn label_c_global(emitter: &mut Emitter, name: &str) {
    abi::emit_c_callback_entry(emitter, name);
}
