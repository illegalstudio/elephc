//! Purpose:
//! Owns inventory node storage and the lifetime of its non-owning resource cell pointer.
//!
//! Called from:
//! - Mixed boxing and resource inventory emission.
//!
//! Key details:
//! - A new node is fully initialized before it is appended to the creation-order list.
//! - Invalidation matches the exact canonical cell before the last owner is destroyed.

use crate::{abi, Arch, Emitter};

/// Offset of the PHP resource identifier.
pub const ID_OFFSET: usize = 8;
/// Offset of the resource's native payload.
pub const PAYLOAD_OFFSET: usize = 16;
/// Offset of the resource subtype.
pub const SUBTYPE_OFFSET: usize = 24;
/// Offset of the node state.
pub const STATE_OFFSET: usize = 32;
/// Offset of the borrowed canonical cell within the native inventory node.
pub const CELL_OFFSET: usize = 40;
/// State of a closed resource whose cell can still be observed.
pub const CLOSED: usize = 1;
/// State of a node whose resource cell has been destroyed.
pub const RETIRED: usize = 2;

const NODE_BYTES: i64 = CELL_OFFSET as i64 + 8;

/// Retains the canonical cell of the inventory node returned by registration.
///
/// The caller stages that node for publication on the allocation path. A missing
/// node or cell branches to allocation, otherwise the shared owner is returned.
pub fn emit_reuse_registered_cell(emitter: &mut impl Emitter, allocate: &str, done: &str) {
    let result = abi::int_result_reg(emitter);
    abi::emit_branch_if_int_result_zero(emitter, allocate);
    abi::emit_load_from_address(emitter, result, result, CELL_OFFSET);
    abi::emit_branch_if_int_result_zero(emitter, allocate);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::emit_jump(emitter, done);
}

/// Allocates and appends a resource node with its identity tuple staged in the registration frame.
///
/// The caller stages id, native payload and subtype in three consecutive words.
/// They start at [sp] on AArch64 and [rbp - 8] on x86_64.
/// Returns the new node in x10 or r10 for the compiler's common registration exit.
pub fn emit_append_node(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(emitter, "x0", NODE_BYTES);
            abi::emit_call_label(emitter, "__rt_heap_alloc");
            emitter.instruction("mov x9, x0");                                  // keep the newly allocated node pointer
            emitter.instruction("str xzr, [x9, #0]");                           // next = null on the new tail
            emitter.instruction("ldr x10, [sp, #0]");                           // reload PHP id
            emitter.instruction(&format!("str x10, [x9, #{ID_OFFSET}]"));       // store PHP id
            emitter.instruction("ldr x10, [sp, #8]");                           // reload native payload
            emitter.instruction(&format!("str x10, [x9, #{PAYLOAD_OFFSET}]"));  // store native payload
            emitter.instruction("ldr x10, [sp, #16]");                          // reload resource subtype
            emitter.instruction(&format!("str x10, [x9, #{SUBTYPE_OFFSET}]"));  // store resource subtype
            emitter.instruction(&format!("str xzr, [x9, #{STATE_OFFSET}]"));    // closed = false
            emitter.instruction(&format!("str xzr, [x9, #{CELL_OFFSET}]"));     // weak canonical cell is bound after boxing
            abi::emit_symbol_address(emitter, "x10", "_resource_inventory_tail");
            emitter.instruction("ldr x11, [x10]");                              // load the preceding tail
            emitter.instruction("cbz x11, __rt_resource_inventory_register_first"); // an empty list initializes its head
            emitter.instruction("str x9, [x11, #0]");                           // append after the preceding tail
            emitter.instruction("b __rt_resource_inventory_register_tail");     // skip first-node head initialization
            emitter.label("__rt_resource_inventory_register_first");
            abi::emit_symbol_address(emitter, "x11", "_resource_inventory_head");
            emitter.instruction("str x9, [x11]");                               // publish the first node as list head
            emitter.label("__rt_resource_inventory_register_tail");
            emitter.instruction("str x9, [x10]");                               // publish the new tail
            emitter.instruction("mov x10, x9");                                 // return the new node through the common found path
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(emitter, "rax", NODE_BYTES);
            abi::emit_call_label(emitter, "__rt_heap_alloc");
            emitter.instruction("mov r9, rax");                                 // keep new node pointer
            emitter.instruction("mov QWORD PTR [r9], 0");                       // next = null
            emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                // reload PHP id
            emitter.instruction(&format!("mov QWORD PTR [r9 + {ID_OFFSET}], r10")); // store PHP id
            emitter.instruction("mov r10, QWORD PTR [rbp - 16]");               // reload native payload
            emitter.instruction(&format!("mov QWORD PTR [r9 + {PAYLOAD_OFFSET}], r10")); // store native payload
            emitter.instruction("mov r10, QWORD PTR [rbp - 24]");               // reload resource subtype
            emitter.instruction(&format!("mov QWORD PTR [r9 + {SUBTYPE_OFFSET}], r10")); // store resource subtype
            emitter.instruction(&format!("mov QWORD PTR [r9 + {STATE_OFFSET}], 0")); // closed = false
            emitter.instruction(&format!("mov QWORD PTR [r9 + {CELL_OFFSET}], 0")); // bind the weak cell after boxing
            abi::emit_symbol_address(emitter, "r10", "_resource_inventory_tail");
            emitter.instruction("mov r11, QWORD PTR [r10]");                    // load preceding tail
            emitter.instruction("test r11, r11");                               // is the list empty?
            emitter.instruction("jz __rt_resource_inventory_register_first_x86"); // initialize head for the first node
            emitter.instruction("mov QWORD PTR [r11], r9");                     // append after preceding tail
            emitter.instruction("jmp __rt_resource_inventory_register_tail_x86"); // skip head initialization
            emitter.label("__rt_resource_inventory_register_first_x86");
            abi::emit_symbol_address(emitter, "r11", "_resource_inventory_head");
            emitter.instruction("mov QWORD PTR [r11], r9");                     // publish first node as head
            emitter.label("__rt_resource_inventory_register_tail_x86");
            emitter.instruction("mov QWORD PTR [r10], r9");                     // publish new tail
            emitter.instruction("mov r10, r9");                                 // return the new node through the common found path
        }
    }
}

/// Publishes a completed box weakly in its resource inventory node.
pub fn emit_publish(emitter: &mut impl Emitter, node_slot: usize, done: &str) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x9, [sp, #{node_slot}]"));        // load the optional inventory node
            emitter.instruction(&format!("cbz x9, {done}"));                    // ordinary values have no inventory identity
            emitter.instruction(&format!("str x0, [x9, #{CELL_OFFSET}]"));      // publish a weak pointer without retaining an immortal owner
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {node_slot}]")); // load the optional inventory node
            emitter.instruction("test r10, r10");                               // ordinary values have no inventory identity
            emitter.instruction(&format!("jz {done}"));                         // return ordinary boxes without registration
            emitter.instruction(&format!("mov QWORD PTR [r10 + {CELL_OFFSET}], rax")); // publish a weak pointer without retaining an immortal owner
        }
    }
}

/// Invalidates the weak inventory entry before the last cell owner runs its destructor.
pub fn emit_retire(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.label_global("__rt_resource_inventory_retire");
    match emitter.arch() {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x9", "_resource_inventory_head");
            emitter.instruction("ldr x9, [x9]");                                // begin weak-cell lookup without changing the input cell
            emitter.label("__rt_resource_inventory_retire_loop");
            emitter.instruction("cbz x9, __rt_resource_inventory_retire_done"); // unregistered resource kinds need no inventory mutation
            emitter.instruction(&format!("ldr x10, [x9, #{CELL_OFFSET}]"));     // inspect the weak canonical cell
            emitter.instruction("cmp x10, x0");                                 // match identity, never a reusable native descriptor
            emitter.instruction("b.eq __rt_resource_inventory_retire_hit");     // invalidate only this incarnation
            emitter.instruction("ldr x9, [x9]");                                // continue to the next node
            emitter.instruction("b __rt_resource_inventory_retire_loop");       // inspect the next weak reference
            emitter.label("__rt_resource_inventory_retire_hit");
            emitter.instruction(&format!("str xzr, [x9, #{CELL_OFFSET}]"));     // clear the weak pointer before storage reclamation
            emitter.instruction(&format!("mov x10, #{RETIRED}"));               // state 2 means the final owner was destroyed
            emitter.instruction(&format!("str x10, [x9, #{STATE_OFFSET}]"));    // enumeration and reset must skip dead resources
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "r10", "_resource_inventory_head");
            emitter.instruction("mov r10, QWORD PTR [r10]");                    // begin weak-cell lookup without changing the input cell
            emitter.label("__rt_resource_inventory_retire_loop");
            emitter.instruction("test r10, r10");                               // detect the end of the weak inventory
            emitter.instruction("jz __rt_resource_inventory_retire_done");      // unregistered resource kinds need no inventory mutation
            emitter.instruction(&format!("cmp QWORD PTR [r10 + {CELL_OFFSET}], rax")); // match cell identity, not a reusable descriptor
            emitter.instruction("je __rt_resource_inventory_retire_hit");       // invalidate only this incarnation
            emitter.instruction("mov r10, QWORD PTR [r10]");                    // continue to the next node
            emitter.instruction("jmp __rt_resource_inventory_retire_loop");     // inspect the next weak reference
            emitter.label("__rt_resource_inventory_retire_hit");
            emitter.instruction(&format!("mov QWORD PTR [r10 + {CELL_OFFSET}], 0")); // clear the weak pointer before storage reclamation
            emitter.instruction(&format!("mov QWORD PTR [r10 + {STATE_OFFSET}], {RETIRED}")); // enumeration and reset must skip dead resources
        }
    }
    emitter.label("__rt_resource_inventory_retire_done");
    emitter.instruction("ret");                                                 // return the unchanged resource cell for its destructor
}

/// Drains the inventory in creation order, reclaiming each node and clearing the list roots.
/// The caller emits cleanup for an open handle in the current node (x9 or r10).
pub fn emit_drain<E: Emitter>(emitter: &mut E, first_resource_id: i64, emit_close_open: impl FnOnce(&mut E)) {
    match emitter.arch() {
        Arch::AArch64 => emit_drain_aarch64(emitter, first_resource_id, emit_close_open),
        Arch::X86_64 => emit_drain_x86_64(emitter, first_resource_id, emit_close_open),
    }
}

/// Walks and frees the native inventory list on AArch64.
fn emit_drain_aarch64<E: Emitter>(emitter: &mut E, first_resource_id: i64, emit_close_open: impl FnOnce(&mut E)) {
    emitter.blank();
    emitter.comment("--- runtime: resource_inventory_reset ---");
    emitter.label_global("__rt_resource_inventory_reset");
    emitter.instruction("sub sp, sp, #32");                                     // reserve an aligned frame and one next-node spill
    emitter.instruction("stp x29, x30, [sp, #16]");                             // preserve caller frame state across heap frees
    emitter.instruction("add x29, sp, #16");                                    // establish the reset frame
    abi::emit_symbol_address(emitter, "x9", "_resource_inventory_head");
    emitter.instruction("ldr x9, [x9]");                                        // start at the oldest node
    emitter.label("__rt_resource_inventory_reset_loop");
    emitter.instruction("cbz x9, __rt_resource_inventory_reset_done");          // finish after the final node
    emitter.instruction("ldr x10, [x9]");                                       // preserve next before freeing this node
    emitter.instruction("str x10, [sp, #0]");                                   // heap free may clobber all scratch registers
    emitter.instruction("str x9, [sp, #8]");                                    // preserve the current node across handle cleanup
    emitter.instruction(&format!("ldr x10, [x9, #{STATE_OFFSET}]"));            // inspect whether an ordinary close already consumed it
    emitter.instruction("cbnz x10, __rt_resource_inventory_reset_free");        // never close one native handle twice
    emit_close_open(emitter);
    emitter.label("__rt_resource_inventory_reset_free");
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the node after cleanup helper calls
    emitter.instruction("mov x0, x9");                                          // free the raw inventory node
    abi::emit_call_label(emitter, "__rt_heap_free");
    emitter.instruction("ldr x9, [sp, #0]");                                    // continue from the saved next pointer
    emitter.instruction("b __rt_resource_inventory_reset_loop");                // process the next incarnation
    emitter.label("__rt_resource_inventory_reset_done");
    abi::emit_store_zero_to_symbol(emitter, "_resource_inventory_head", 0);
    abi::emit_store_zero_to_symbol(emitter, "_resource_inventory_tail", 0);
    abi::emit_load_int_immediate(emitter, "x9", first_resource_id);
    abi::emit_store_reg_to_symbol(emitter, "x9", "_resource_id_next", 0);
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore caller frame state
    emitter.instruction("add sp, sp, #32");                                     // release reset storage
    emitter.instruction("ret");                                                 // return with an empty inventory
}

/// Walks and frees the native inventory list on x86_64.
fn emit_drain_x86_64<E: Emitter>(emitter: &mut E, first_resource_id: i64, emit_close_open: impl FnOnce(&mut E)) {
    emitter.blank();
    emitter.comment("--- runtime: resource_inventory_reset ---");
    emitter.label_global("__rt_resource_inventory_reset");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer and align nested calls
    emitter.instruction("mov rbp, rsp");                                        // establish the reset frame
    emitter.instruction("sub rsp, 16");                                         // reserve one next-node spill while keeping alignment
    abi::emit_symbol_address(emitter, "r10", "_resource_inventory_head");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // start at the oldest node
    emitter.label("__rt_resource_inventory_reset_loop_x86");
    emitter.instruction("test r10, r10");                                       // did the list end?
    emitter.instruction("jz __rt_resource_inventory_reset_done_x86");           // finish after the final node
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // preserve next before freeing this node
    emitter.instruction("mov QWORD PTR [rbp - 8], r11");                        // heap free may clobber all scratch registers
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // preserve the current node across handle cleanup
    emitter.instruction(&format!("cmp QWORD PTR [r10 + {STATE_OFFSET}], 0"));   // inspect whether an ordinary close already consumed it
    emitter.instruction("jne __rt_resource_inventory_reset_free_x86");          // never close one native handle twice
    emit_close_open(emitter);
    emitter.label("__rt_resource_inventory_reset_free_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the node after cleanup helper calls
    emitter.instruction("mov rax, r10");                                        // free the raw inventory node
    abi::emit_call_label(emitter, "__rt_heap_free");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // continue from the saved next pointer
    emitter.instruction("jmp __rt_resource_inventory_reset_loop_x86");          // process the next incarnation
    emitter.label("__rt_resource_inventory_reset_done_x86");
    abi::emit_store_zero_to_symbol(emitter, "_resource_inventory_head", 0);
    abi::emit_store_zero_to_symbol(emitter, "_resource_inventory_tail", 0);
    abi::emit_load_int_immediate(emitter, "r10", first_resource_id);
    abi::emit_store_reg_to_symbol(emitter, "r10", "_resource_id_next", 0);
    emitter.instruction("leave");                                               // release reset storage and restore rbp
    emitter.instruction("ret");                                                 // return with an empty inventory
}

/// Size of the native payload-to-resource-id side table. Probing masks with capacity minus one.
pub const RESOURCE_ID_TABLE_SLOTS: usize = 1024;

/// Declares the native resource identity tables and linked weak inventory roots.
pub fn append_storage(out: &mut String, target: &impl crate::data::DataTarget) {
    for symbol in ["_resource_id_keys", "_resource_id_vals"] {
        out.push_str(&target.common(symbol, RESOURCE_ID_TABLE_SLOTS * 8));
    }
    for symbol in ["_resource_inventory_head", "_resource_inventory_tail"] {
        out.push_str(&target.common(symbol, 8));
    }
}

/// Finds the weak incarnation node for a resource id before optional registration.
pub fn emit_find_registration_aarch64(emitter: &mut impl Emitter) {
    emitter.emit_symbol_address("x9", "_resource_inventory_head");
    emitter.instruction("ldr x10, [x9]");                                       // load the first recorded incarnation
    emitter.label("__rt_resource_inventory_register_find");
    emitter.instruction("cbz x10, __rt_resource_inventory_register_new");       // a missing id needs a fresh node
    emitter.instruction(&format!("ldr x11, [x10, #{ID_OFFSET}]"));              // load the node's PHP id
    emitter.instruction("cmp x11, x0");                                         // compare with the incoming PHP id
    emitter.instruction("b.eq __rt_resource_inventory_register_found");         // aliases share the existing incarnation node
    emitter.instruction("ldr x10, [x10, #0]");                                  // follow the creation-order next pointer
    emitter.instruction("b __rt_resource_inventory_register_find");             // continue until the id is found or the list ends
}

/// Finds the weak incarnation node for a resource id before optional registration.
pub fn emit_find_registration_x86_64(emitter: &mut impl Emitter) {
    emitter.emit_symbol_address("r10", "_resource_inventory_head");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // load first node
    emitter.label("__rt_resource_inventory_register_find_x86");
    emitter.instruction("test r10, r10");                                       // did the list end?
    emitter.instruction("jz __rt_resource_inventory_register_new_x86");         // allocate a node for a new id
    emitter.instruction(&format!("mov r11, QWORD PTR [r10 + {ID_OFFSET}]"));    // load stored PHP id
    emitter.instruction("cmp r11, QWORD PTR [rbp - 8]");                        // compare with incoming id
    emitter.instruction("je __rt_resource_inventory_register_found_x86");       // aliases share the existing incarnation node
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // follow next pointer
    emitter.instruction("jmp __rt_resource_inventory_register_find_x86");       // continue search
}

/// Locates the weak incarnation and publishes its closed state before returning.
pub fn emit_mark_closed_aarch64(emitter: &mut impl Emitter) {
    emitter.emit_symbol_address("x9", "_resource_inventory_head");
    emitter.instruction("ldr x9, [x9]");                                        // start at the oldest resource incarnation
    emitter.label("__rt_resource_inventory_close_find");
    emitter.instruction("cbz x9, __rt_resource_inventory_close_done");          // an untracked id needs no update
    emitter.instruction(&format!("ldr x10, [x9, #{ID_OFFSET}]"));               // load the node's PHP id
    emitter.instruction("cmp x10, x0");                                         // is this the resource being closed?
    emitter.instruction("b.eq __rt_resource_inventory_close_hit");              // update the matching node
    emitter.instruction("ldr x9, [x9, #0]");                                    // follow the next pointer
    emitter.instruction("b __rt_resource_inventory_close_find");                // continue the linear search
    emitter.label("__rt_resource_inventory_close_hit");
    emitter.instruction(&format!("mov x10, #{CLOSED}"));                        // materialize the closed marker
    emitter.instruction(&format!("str x10, [x9, #{STATE_OFFSET}]"));            // retain the node but mark it Unknown
    emitter.instruction("neg x10, x0");                                         // closed resources carry their stable id as -id
    emitter.instruction(&format!("str x10, [x9, #{PAYLOAD_OFFSET}]"));          // replace the now-invalid native payload
}

/// Locates the weak incarnation and publishes its closed state before returning.
pub fn emit_mark_closed_x86_64(emitter: &mut impl Emitter) {
    emitter.emit_symbol_address("r10", "_resource_inventory_head");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // start at oldest incarnation
    emitter.label("__rt_resource_inventory_close_find_x86");
    emitter.instruction("test r10, r10");                                       // did the list end?
    emitter.instruction("jz __rt_resource_inventory_close_done_x86");           // an untracked id is a no-op
    emitter.instruction(&format!("cmp QWORD PTR [r10 + {ID_OFFSET}], rax"));    // compare node id with the closing id
    emitter.instruction("je __rt_resource_inventory_close_hit_x86");            // update matching node
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // follow next pointer
    emitter.instruction("jmp __rt_resource_inventory_close_find_x86");          // continue search
    emitter.label("__rt_resource_inventory_close_hit_x86");
    emitter.instruction(&format!("mov QWORD PTR [r10 + {STATE_OFFSET}], {CLOSED}")); // mark resource Unknown
    emitter.instruction("mov r11, rax");                                        // copy stable PHP id
    emitter.instruction("neg r11");                                             // closed payload sentinel = -id
    emitter.instruction(&format!("mov QWORD PTR [r10 + {PAYLOAD_OFFSET}], r11")); // discard the invalid native payload
}

/// Synchronizes the subtype or closed state by canonical cell address.
/// The caller already closed the native handle when the subtype argument is negative.
pub fn emit_update_canonical_cell(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(emitter, "x9", "_resource_inventory_head", 0);
            emitter.label("__elephc_eval_resource_state_next");
            emitter.instruction("cbz x9, __elephc_eval_resource_state_done");   // an untracked or retired cell requires no update
            emitter.instruction(&format!("ldr x10, [x9, #{CELL_OFFSET}]"));     // compare the canonical identity, never a recycled handle
            emitter.instruction("cmp x10, x0");                                 // is this the resource's weak inventory node?
            emitter.instruction("b.eq __elephc_eval_resource_state_found");     // update its shared metadata
            emitter.instruction("ldr x9, [x9]");                                // inspect the next incarnation
            emitter.instruction("b __elephc_eval_resource_state_next");         // continue identity lookup
            emitter.label("__elephc_eval_resource_state_found");
            emitter.instruction("cmp x1, #0");                                  // negative subtype requests an explicit close
            emitter.instruction("b.lt __elephc_eval_resource_state_closed");    // preserve the ID in a closed sentinel
            emitter.instruction(&format!("str x1, [x9, #{SUBTYPE_OFFSET}]"));   // publish the eval resource's exact PHP subtype
            crate::layout::emit_store_mixed_high(emitter, "x1", "x0");          // keep canonical boxed type metadata in sync
            emitter.instruction("b __elephc_eval_resource_state_done");         // complete an open resource update
            emitter.label("__elephc_eval_resource_state_closed");
            emitter.instruction(&format!("mov x10, #{CLOSED}"));                // mark this incarnation closed
            emitter.instruction(&format!("str x10, [x9, #{STATE_OFFSET}]"));    // exclude it from open-resource filters
            emitter.instruction(&format!("ldr x10, [x9, #{ID_OFFSET}]"));       // read its stable PHP ID
            emitter.instruction("neg x10, x10");                                // encode the closed identity as a negative payload
            emitter.instruction(&format!("str x10, [x9, #{PAYLOAD_OFFSET}]"));  // invalidate the inventory's raw handle
            crate::layout::emit_store_mixed_low(emitter, "x10", "x0");          // all aliases observe the same closed state
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(emitter, "r10", "_resource_inventory_head", 0);
            emitter.label("__elephc_eval_resource_state_next");
            emitter.instruction("test r10, r10");                               // detect the end of the weak inventory
            emitter.instruction("jz __elephc_eval_resource_state_done");        // no update for untracked or retired cells
            emitter.instruction(&format!("cmp QWORD PTR [r10 + {CELL_OFFSET}], rdi")); // match canonical identity, never a recycled descriptor
            emitter.instruction("je __elephc_eval_resource_state_found");       // update shared metadata
            emitter.instruction("mov r10, QWORD PTR [r10]");                    // inspect the next incarnation
            emitter.instruction("jmp __elephc_eval_resource_state_next");       // continue identity lookup
            emitter.label("__elephc_eval_resource_state_found");
            emitter.instruction("test rsi, rsi");                               // negative subtype requests an explicit close
            emitter.instruction("js __elephc_eval_resource_state_closed");      // preserve the ID in a closed sentinel
            emitter.instruction(&format!("mov QWORD PTR [r10 + {SUBTYPE_OFFSET}], rsi")); // publish the precise eval resource subtype
            crate::layout::emit_store_mixed_high(emitter, "rsi", "rdi");        // keep the canonical cell consistent
            emitter.instruction("jmp __elephc_eval_resource_state_done");       // complete an open resource update
            emitter.label("__elephc_eval_resource_state_closed");
            emitter.instruction(&format!("mov QWORD PTR [r10 + {STATE_OFFSET}], {CLOSED}")); // exclude closed resources from open filters
            emitter.instruction(&format!("mov r11, QWORD PTR [r10 + {ID_OFFSET}]")); // read the stable PHP resource ID
            emitter.instruction("neg r11");                                     // encode closed identity as a negative payload
            emitter.instruction(&format!("mov QWORD PTR [r10 + {PAYLOAD_OFFSET}], r11")); // invalidate the inventory's raw handle
            crate::layout::emit_store_mixed_low(emitter, "r11", "rdi");         // update every retained alias through its canonical cell
        }
    }
    emitter.label("__elephc_eval_resource_state_done");
    emitter.instruction("ret");                                                 // the eval resource owner has already performed the actual close
}

/// Partitions the inventory, publishes the live chain, and reclaims every detached orphan.
/// The callback receives an open orphan's subtype in the native result register.
/// It may close the handle and publish new resources before the node itself is reclaimed.
pub fn emit_prune<E: Emitter>(emitter: &mut E, emit_close_orphan: impl FnOnce(&mut E)) {
    let value = emitter.int_result_reg();
    let scratch = emitter.secondary_scratch_reg();
    emitter.emit_load_symbol_to_reg(value, "_resource_inventory_head", 0);
    emitter.store_at_offset(value, 8);
    for offset in [16, 24, 32, 40] { emitter.emit_store_zero_to_local_slot(offset); }
    emitter.label("__rt_web_inventory_scan");
    emitter.load_at_offset(value, 8);
    emitter.emit_branch_if_int_result_zero("__rt_web_inventory_publish");
    emitter.store_at_offset(value, 48);
    emitter.emit_load_from_address(scratch, value, 0);
    emitter.store_at_offset(scratch, 8);
    emitter.emit_store_zero_to_address(value, 0);
    emitter.emit_load_from_address(value, value, STATE_OFFSET);
    emitter.emit_branch_if_int_result_nonzero("__rt_web_inventory_orphan");
    emitter.load_at_offset(value, 48);
    emitter.emit_load_from_address(value, value, CELL_OFFSET);
    emitter.emit_branch_if_int_result_zero("__rt_web_inventory_orphan");
    append(emitter, 24, 32, "live");
    emitter.emit_jump("__rt_web_inventory_scan");
    emitter.label("__rt_web_inventory_orphan");
    append(emitter, 16, 40, "dead");
    emitter.emit_jump("__rt_web_inventory_scan");

    // -- make retained resources visible before close hooks can inspect the inventory --
    emitter.label("__rt_web_inventory_publish");
    for (offset, symbol) in [(24, "_resource_inventory_head"), (32, "_resource_inventory_tail")] {
        emitter.load_at_offset(value, offset);
        emitter.emit_store_reg_to_symbol(value, symbol, 0);
    }
    emitter.load_at_offset(value, 16);
    emitter.store_at_offset(value, 8);
    emitter.label("__rt_web_inventory_retire");
    emitter.load_at_offset(value, 8);
    emitter.emit_branch_if_int_result_zero("__rt_web_inventory_done");
    emitter.store_at_offset(value, 48);
    emitter.emit_load_from_address(scratch, value, 0);
    emitter.store_at_offset(scratch, 8);
    emitter.emit_store_zero_to_address(value, 0);
    emitter.emit_load_from_address(value, value, STATE_OFFSET);
    emitter.emit_branch_if_int_result_nonzero("__rt_web_inventory_free");
    emitter.load_at_offset(value, 48);
    emitter.emit_load_from_address(value, value, SUBTYPE_OFFSET);
    emit_close_orphan(emitter);

    // -- isolate the legacy node free without losing callback-created resources or IDs --
    emitter.label("__rt_web_inventory_free");
    for (offset, symbol) in [(56, "_resource_inventory_head"), (64, "_resource_inventory_tail"), (72, "_resource_id_next")] {
        emitter.emit_load_symbol_to_reg(value, symbol, 0);
        emitter.store_at_offset(value, offset);
    }
    emitter.load_at_offset(value, 48);
    emitter.emit_store_reg_to_symbol(value, "_resource_inventory_head", 0);
    emitter.emit_store_reg_to_symbol(value, "_resource_inventory_tail", 0);
    emitter.emit_call_label("__rt_resource_inventory_reset");
    for (offset, symbol) in [(56, "_resource_inventory_head"), (64, "_resource_inventory_tail"), (72, "_resource_id_next")] {
        emitter.load_at_offset(value, offset);
        emitter.emit_store_reg_to_symbol(value, symbol, 0);
    }
    emitter.emit_jump("__rt_web_inventory_retire");
}

/// Appends the node in slot 48 to a local chain without allocating or invoking PHP.
fn append(emitter: &mut impl Emitter, head: usize, tail: usize, suffix: &str) {
    let value = abi::int_result_reg(emitter);
    let node = abi::secondary_scratch_reg(emitter);
    let first = format!("__rt_web_inventory_{suffix}_first");
    let done = format!("__rt_web_inventory_{suffix}_appended");
    emitter.load_at_offset(value, tail);
    emitter.emit_branch_if_int_result_zero(&first);
    emitter.load_at_offset(node, 48);
    emitter.emit_store_to_address(node, value, 0);
    emitter.emit_jump(&done);
    emitter.label(&first);
    emitter.load_at_offset(node, 48);
    emitter.store_at_offset(node, head);
    emitter.label(&done);
    emitter.store_at_offset(node, tail);
}

/// Loads the current orphan's native payload after a close policy checks its subtype.
pub fn emit_load_orphan_payload(emitter: &mut impl Emitter) {
    let value = emitter.int_result_reg();
    emitter.load_at_offset(value, 48);
    emitter.emit_load_from_address(value, value, PAYLOAD_OFFSET);
}

/// Marks the current orphan closed before calling a potentially reentrant close hook.
pub fn emit_mark_orphan_closed(emitter: &mut impl Emitter) {
    let node = emitter.secondary_scratch_reg();
    emitter.load_at_offset(node, 48);
    let flag = if emitter.arch() == Arch::AArch64 { "x12" } else { "r11" };
    emitter.emit_load_int_immediate(flag, CLOSED as i64);
    emitter.emit_store_to_address(flag, node, STATE_OFFSET);
}
