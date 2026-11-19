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
