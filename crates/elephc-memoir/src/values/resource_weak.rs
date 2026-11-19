//! Purpose:
//! Publishes and retires the inventory's non-owning resource cell pointer.
//!
//! Called from:
//! - Mixed boxing and resource inventory emission.
//!
//! Key details:
//! - Invalidation matches the exact canonical cell before the last owner is destroyed.

use crate::{abi, Arch, Emitter};

/// Offset of the borrowed canonical cell within the native inventory node.
pub const CELL_OFFSET: usize = 40;
const STATE_OFFSET: usize = 32;
const RETIRED: usize = 2;

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
