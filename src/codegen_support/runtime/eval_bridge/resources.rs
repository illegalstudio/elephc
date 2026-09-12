//! Purpose:
//! Exposes the shared resource inventory as an owned eval-compatible Mixed snapshot.
//!
//! Called from:
//! - `super::emit_eval_bridge_runtime()` when Magician support is linked.
//!
//! Key details:
//! - The filter selector follows __rt_get_resources; resource entries retain canonical cells.
//! - The wrapper adopts the returned hash instead of introducing another owner.

use super::*;
use crate::codegen_support::emit_box_current_owned_value_as_mixed;
use crate::types::PhpType;

/// Adapts the native inventory's C ABI result into the boxed-cell ABI used by eval.
pub(super) fn emit_resource_inventory_wrapper(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_resource_inventory");
    abi::emit_frame_prologue(emitter, 16);
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rax, rdi");                                    // resource inventory consumes its selector in the result register
    }
    abi::emit_call_label(emitter, "__rt_get_resources");
    emit_box_current_owned_value_as_mixed(emitter, &PhpType::AssocArray {
        key: Box::new(PhpType::Int), value: Box::new(PhpType::Mixed),
    });
    abi::emit_frame_restore(emitter, 16);
    emitter.instruction("ret");                                                 // transfer ownership of the boxed inventory to Magician
    emit_resource_state_wrapper(emitter);
}

/// Updates an eval resource by canonical cell identity without taking ownership of its handle.
fn emit_resource_state_wrapper(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_resource_state");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(emitter, "x9", "_resource_inventory_head", 0);
            emitter.label("__elephc_eval_resource_state_next");
            emitter.instruction("cbz x9, __elephc_eval_resource_state_done");   // an untracked or retired cell requires no update
            emitter.instruction("ldr x10, [x9, #40]");                          // compare the canonical identity, never a recycled handle
            emitter.instruction("cmp x10, x0");                                 // is this the resource's weak inventory node?
            emitter.instruction("b.eq __elephc_eval_resource_state_found");     // update its shared metadata
            emitter.instruction("ldr x9, [x9]");                                // inspect the next incarnation
            emitter.instruction("b __elephc_eval_resource_state_next");         // continue identity lookup
            emitter.label("__elephc_eval_resource_state_found");
            emitter.instruction("cmp x1, #0");                                  // negative subtype requests an explicit close
            emitter.instruction("b.lt __elephc_eval_resource_state_closed");    // preserve the ID in a closed sentinel
            emitter.instruction("str x1, [x9, #24]");                           // publish the eval resource's exact PHP subtype
            emitter.instruction("str x1, [x0, #16]");                           // keep canonical boxed type metadata in sync
            emitter.instruction("b __elephc_eval_resource_state_done");         // complete an open resource update
            emitter.label("__elephc_eval_resource_state_closed");
            emitter.instruction("mov x10, #1");                                 // mark this incarnation closed
            emitter.instruction("str x10, [x9, #32]");                          // exclude it from open-resource filters
            emitter.instruction("ldr x10, [x9, #8]");                           // read its stable PHP ID
            emitter.instruction("neg x10, x10");                                // encode the closed identity as a negative payload
            emitter.instruction("str x10, [x9, #16]");                          // invalidate the inventory's raw handle
            emitter.instruction("str x10, [x0, #8]");                           // all aliases observe the same closed state
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(emitter, "r10", "_resource_inventory_head", 0);
            emitter.label("__elephc_eval_resource_state_next");
            emitter.instruction("test r10, r10");                               // detect the end of the weak inventory
            emitter.instruction("jz __elephc_eval_resource_state_done");        // no update for untracked or retired cells
            emitter.instruction("cmp QWORD PTR [r10 + 40], rdi");               // match canonical identity, never a recycled descriptor
            emitter.instruction("je __elephc_eval_resource_state_found");       // update shared metadata
            emitter.instruction("mov r10, QWORD PTR [r10]");                    // inspect the next incarnation
            emitter.instruction("jmp __elephc_eval_resource_state_next");       // continue identity lookup
            emitter.label("__elephc_eval_resource_state_found");
            emitter.instruction("test rsi, rsi");                               // negative subtype requests an explicit close
            emitter.instruction("js __elephc_eval_resource_state_closed");      // preserve the ID in a closed sentinel
            emitter.instruction("mov QWORD PTR [r10 + 24], rsi");               // publish the precise eval resource subtype
            emitter.instruction("mov QWORD PTR [rdi + 16], rsi");               // keep the canonical cell consistent
            emitter.instruction("jmp __elephc_eval_resource_state_done");       // complete an open resource update
            emitter.label("__elephc_eval_resource_state_closed");
            emitter.instruction("mov QWORD PTR [r10 + 32], 1");                 // exclude closed resources from open filters
            emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                // read the stable PHP resource ID
            emitter.instruction("neg r11");                                     // encode closed identity as a negative payload
            emitter.instruction("mov QWORD PTR [r10 + 16], r11");               // invalidate the inventory's raw handle
            emitter.instruction("mov QWORD PTR [rdi + 8], r11");                // update every retained alias through its canonical cell
        }
    }
    emitter.label("__elephc_eval_resource_state_done");
    emitter.instruction("ret");                                                 // the eval resource owner has already performed the actual close
}
