//! Purpose:
//! Owns the heap header and the entry nodes of a materialized native eval scope.
//!
//! Called from:
//! - The compiler's scope-only eval C ABI.
//!
//! Key details:
//! - Name bytes are borrowed static data.
//! - Owned cells retire before their nodes. Scope flags and lookup stay with the caller.
//! - Suspended-frame collection visits the exact non-null owned cell fields without mutation.

use crate::{Arch, Emitter};

/// Emits the C ABI allocator and destructor for a scope and its owned entries.
pub fn emit_lifecycle(emitter: &mut impl Emitter, owned_flag: i64) {
    match emitter.arch() {
        Arch::AArch64 => {
            emit_aarch64_eval_scope_new(emitter);
            emit_aarch64_eval_scope_free(emitter, owned_flag);
            emit_aarch64_eval_scope_owner_visitor(emitter, owned_flag);
        }
        Arch::X86_64 => {
            emit_x86_64_eval_scope_new(emitter);
            emit_x86_64_eval_scope_free(emitter, owned_flag);
            emit_x86_64_eval_scope_owner_visitor(emitter, owned_flag);
        }
    }
}

/// Publishes a new node using the native scope setter's staged operands.
pub fn emit_insert(emitter: &mut impl Emitter, visible_flags: i64) {
    match emitter.arch() {
        Arch::AArch64 => aarch64_insert(emitter, visible_flags),
        Arch::X86_64 => x86_64_insert(emitter, visible_flags),
    }
}

/// Emits an exported label in the C ABI's target spelling.
fn label_c_global(emitter: &mut impl Emitter, label: &str) {
    emitter.label_global(&emitter.extern_symbol(label));
}

/// Emits the ARM64 scope allocator.
fn emit_aarch64_eval_scope_new(emitter: &mut impl Emitter) {
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
fn emit_aarch64_eval_scope_free(emitter: &mut impl Emitter, owned_flag: i64) {
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
    emitter.instruction(&format!("tst x9, #{}", owned_flag));                   // check whether the scope owns this Mixed cell
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

/// Emits the ARM64 pure visitor for exact cell fields owned by a scope-only eval map.
fn emit_aarch64_eval_scope_owner_visitor(emitter: &mut impl Emitter, owned_flag: i64) {
    label_c_global(emitter, "__elephc_eval_scope_visit_owners_v1");
    emitter.instruction("cbz x0, __elephc_eval_scope_visit_owners_v1_done");    // a null scope has no owner fields
    emitter.instruction("cbz x1, __elephc_eval_scope_visit_owners_v1_done");    // a null callback requests no traversal
    emitter.instruction("sub sp, sp, #48");                                     // reserve an aligned frame for stable scan state
    emitter.instruction("stp x29, x30, [sp, #0]");                              // preserve frame pointer and return address across callbacks
    emitter.instruction("stp x19, x20, [sp, #16]");                             // preserve current entry and callback registers
    emitter.instruction("stp x21, x22, [sp, #32]");                             // preserve callback context and alignment padding
    emitter.instruction("mov x29, sp");                                         // establish a stable callback frame
    emitter.instruction("ldr x19, [x0]");                                       // load the first scope entry
    emitter.instruction("mov x20, x1");                                         // keep the visitor callback across its own calls
    emitter.instruction("mov x21, x2");                                         // keep the opaque visitor context unchanged
    emitter.label("__elephc_eval_scope_visit_owners_v1_loop");
    emitter.instruction("cbz x19, __elephc_eval_scope_visit_owners_v1_exit");   // finish after the final entry
    emitter.instruction("ldr x9, [x19, #32]");                                  // load entry ownership flags
    emitter.instruction(&format!("tst x9, #{}", owned_flag));                   // borrowed entries add no collector edge
    emitter.instruction("b.eq __elephc_eval_scope_visit_owners_v1_next");       // skip borrowed storage
    emitter.instruction("ldr x9, [x19, #24]");                                  // read the owned cell without retaining it
    emitter.instruction("cbz x9, __elephc_eval_scope_visit_owners_v1_next");    // vacant owner fields contribute no edge
    emitter.instruction("add x0, x19, #24");                                    // pass the exact address of entry->cell
    emitter.instruction("mov x1, #0");                                          // ordinary Mixed owner fields use tag zero
    emitter.instruction("mov x2, x21");                                         // forward the opaque visitor context
    emitter.instruction("blr x20");                                             // record the owner without running PHP
    emitter.label("__elephc_eval_scope_visit_owners_v1_next");
    emitter.instruction("ldr x19, [x19]");                                      // advance through the immutable entry list
    emitter.instruction("b __elephc_eval_scope_visit_owners_v1_loop");          // inspect the next scope entry
    emitter.label("__elephc_eval_scope_visit_owners_v1_exit");
    emitter.instruction("ldp x21, x22, [sp, #32]");                             // restore callback context registers
    emitter.instruction("ldp x19, x20, [sp, #16]");                             // restore scan registers
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore the caller frame and return address
    emitter.instruction("add sp, sp, #48");                                     // release the visitor frame
    emitter.label("__elephc_eval_scope_visit_owners_v1_done");
    emitter.instruction("ret");                                                 // return without mutating scope ownership
}

/// Allocates and publishes a node using the ARM64 setter frame.
fn aarch64_insert(emitter: &mut impl Emitter, visible_flags: i64) {
    emitter.label("__elephc_eval_scope_set_insert");
    emitter.instruction("mov x0, #40");                                         // entry records store next, name, length, cell, and flags
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate a new entry record
    emitter.instruction("mov x26, x0");                                         // keep the new entry pointer while initializing fields
    emitter.instruction("str x25, [x26]");                                      // new entry next points at the old successor, normally null
    emitter.instruction("str x20, [x26, #8]");                                  // store the borrowed static name pointer
    emitter.instruction("str x21, [x26, #16]");                                 // store the name byte length
    emitter.instruction("str x22, [x26, #24]");                                 // store the Mixed cell pointer
    emitter.instruction(&format!("mov x9, #{}", visible_flags));                // materialize visible and dirty ABI bits
    emitter.instruction("orr x9, x23, x9");                                     // merge caller ownership flags with core visibility flags
    emitter.instruction("str x9, [x26, #32]");                                  // store the new entry flags
    emitter.instruction("str x26, [x24]");                                      // link the new entry through the previous-next pointer
}

/// Emits the x86_64 scope allocator.
fn emit_x86_64_eval_scope_new(emitter: &mut impl Emitter) {
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
fn emit_x86_64_eval_scope_free(emitter: &mut impl Emitter, owned_flag: i64) {
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
    emitter.instruction(&format!("test r11, {}", owned_flag));                  // check whether the scope owns this Mixed cell
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

/// Emits the x86_64 pure visitor for exact cell fields owned by a scope-only eval map.
fn emit_x86_64_eval_scope_owner_visitor(emitter: &mut impl Emitter, owned_flag: i64) {
    label_c_global(emitter, "__elephc_eval_scope_visit_owners_v1");
    emitter.instruction("test rdi, rdi");                                       // a null scope has no owner fields
    emitter.instruction("jz __elephc_eval_scope_visit_owners_v1_done");         // return before building a frame
    emitter.instruction("test rsi, rsi");                                       // a null callback requests no traversal
    emitter.instruction("jz __elephc_eval_scope_visit_owners_v1_done");         // leave the scope untouched
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable aligned callback frame
    emitter.instruction("sub rsp, 32");                                         // reserve callee-saved scan state
    emitter.instruction("mov QWORD PTR [rbp - 8], r12");                        // preserve the current-entry register
    emitter.instruction("mov QWORD PTR [rbp - 16], r13");                       // preserve the callback register
    emitter.instruction("mov QWORD PTR [rbp - 24], r14");                       // preserve the callback context register
    emitter.instruction("mov r12, QWORD PTR [rdi]");                            // load the first scope entry
    emitter.instruction("mov r13, rsi");                                        // keep the callback across its own calls
    emitter.instruction("mov r14, rdx");                                        // keep the opaque visitor context unchanged
    emitter.label("__elephc_eval_scope_visit_owners_v1_loop");
    emitter.instruction("test r12, r12");                                       // has the scan reached the list tail?
    emitter.instruction("jz __elephc_eval_scope_visit_owners_v1_exit");         // finish after the final entry
    emitter.instruction("mov rax, QWORD PTR [r12 + 32]");                       // load entry ownership flags
    emitter.instruction(&format!("test rax, {}", owned_flag));                  // borrowed entries add no collector edge
    emitter.instruction("jz __elephc_eval_scope_visit_owners_v1_next");         // skip borrowed storage
    emitter.instruction("mov rax, QWORD PTR [r12 + 24]");                       // read the owned cell without retaining it
    emitter.instruction("test rax, rax");                                       // vacant owner fields contribute no edge
    emitter.instruction("jz __elephc_eval_scope_visit_owners_v1_next");         // skip the null cell
    emitter.instruction("lea rdi, [r12 + 24]");                                 // pass the exact address of entry->cell
    emitter.instruction("xor esi, esi");                                        // ordinary Mixed owner fields use tag zero
    emitter.instruction("mov rdx, r14");                                        // forward the opaque visitor context
    emitter.instruction("call r13");                                            // record the owner without running PHP
    emitter.label("__elephc_eval_scope_visit_owners_v1_next");
    emitter.instruction("mov r12, QWORD PTR [r12]");                            // advance through the immutable entry list
    emitter.instruction("jmp __elephc_eval_scope_visit_owners_v1_loop");        // inspect the next scope entry
    emitter.label("__elephc_eval_scope_visit_owners_v1_exit");
    emitter.instruction("mov r12, QWORD PTR [rbp - 8]");                        // restore the caller's scan register
    emitter.instruction("mov r13, QWORD PTR [rbp - 16]");                       // restore the caller's callback register
    emitter.instruction("mov r14, QWORD PTR [rbp - 24]");                       // restore the caller's context register
    emitter.instruction("mov rsp, rbp");                                        // release visitor-local storage
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.label("__elephc_eval_scope_visit_owners_v1_done");
    emitter.instruction("ret");                                                 // return without mutating scope ownership
}

/// Allocates and publishes a node using the x86_64 setter frame.
fn x86_64_insert(emitter: &mut impl Emitter, visible_flags: i64) {
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
    emitter.instruction(&format!("or r10, {}", visible_flags));                 // mark the new entry visible and dirty
    emitter.instruction("mov QWORD PTR [rax + 32], r10");                       // store the new entry flags
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // reload the previous-next pointer
    emitter.instruction("mov QWORD PTR [r11], rax");                            // link the new entry through the previous-next pointer
}

/// Replaces the current node, retiring an old owned cell when its identity changes.
/// The setter supplies its ownership and visible ABI flags.
pub fn emit_replace(emitter: &mut impl Emitter, owned_flag: i64, visible_flags: i64) {
    match emitter.arch() {
        Arch::AArch64 => replace_aarch64(emitter, owned_flag, visible_flags),
        Arch::X86_64 => replace_x86_64(emitter, owned_flag, visible_flags),
    }
}

/// Replaces the selected node using the setter's existing saved state.
fn replace_aarch64(emitter: &mut impl Emitter, owned_flag: i64, visible_flags: i64) {
    emitter.label("__elephc_eval_scope_set_update");
    emitter.instruction("ldr x9, [x25, #32]");                                  // load old entry flags
    emitter.instruction(&format!("tst x9, #{}", owned_flag));                   // check whether the old cell is scope-owned
    emitter.instruction("b.eq __elephc_eval_scope_set_store");                  // borrowed old cells do not need release
    emitter.instruction("ldr x0, [x25, #24]");                                  // load the old Mixed cell pointer
    emitter.instruction("cmp x0, x22");                                         // compare old and replacement cells
    emitter.instruction("b.eq __elephc_eval_scope_set_store");                  // retaining the same cell must not decref it
    emitter.instruction("cbz x0, __elephc_eval_scope_set_store");               // tolerate null old cells defensively
    emitter.instruction("bl __rt_decref_mixed");                                // release the overwritten owned Mixed cell
    emitter.label("__elephc_eval_scope_set_store");
    emitter.instruction("str x22, [x25, #24]");                                 // store the replacement Mixed cell
    emitter.instruction(&format!("mov x9, #{}", visible_flags));                // materialize visible and dirty ABI bits
    emitter.instruction("orr x9, x23, x9");                                     // merge caller ownership flags with core visibility flags
    emitter.instruction("str x9, [x25, #32]");                                  // publish the updated ABI flags
}

/// Replaces the selected node using the setter's existing saved state.
fn replace_x86_64(emitter: &mut impl Emitter, owned_flag: i64, visible_flags: i64) {
    emitter.label("__elephc_eval_scope_set_update");
    emitter.instruction("mov r10, QWORD PTR [r9 + 32]");                        // load old entry flags
    emitter.instruction(&format!("test r10, {}", owned_flag));                  // check whether the old cell is scope-owned
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
    emitter.instruction(&format!("or r10, {}", visible_flags));                 // mark the entry visible and dirty
    emitter.instruction("mov QWORD PTR [r9 + 32], r10");                        // publish the updated ABI flags
}
