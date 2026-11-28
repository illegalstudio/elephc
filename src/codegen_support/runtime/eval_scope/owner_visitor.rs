//! Purpose:
//! Exposes scope-only AOT eval owners to suspended native frame cycle collection.
//!
//! Called from:
//! - Core scope runtime emission and generated saved-frame ownership visitors.
//!
//! Key details:
//! - The callback sees exact owning pointer-field addresses with the ordinary tag-zero ABI.
//! - Native contract cells and linked-list entry owners remain independent graph edges.
//! - Traversal cannot execute PHP, change ownership or mutate the scope.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

const FRAME: usize = 64;
const SCOPE: usize = 8;
const VISITOR: usize = 16;
const CONTEXT: usize = 24;
const ENTRY: usize = 32;

/// Walks actual owning slots without retaining a value or collapsing equal pointer identities.
pub(super) fn emit(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::symbol_scratch_reg(emitter);
    let done = "__rt_eval_scope_visit_owners_done";
    let next = "__rt_eval_scope_visit_owners_next";
    let scan = "__rt_eval_scope_visit_owners_scan";
    emitter.label_global(&emitter.target.extern_symbol("__elephc_eval_scope_visit_owners_v1"));
    abi::emit_frame_prologue(emitter, FRAME);
    for (index, offset) in [SCOPE, VISITOR, CONTEXT].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    abi::load_at_offset(emitter, result, SCOPE);
    abi::emit_branch_if_int_result_zero(emitter, done);
    abi::load_at_offset(emitter, result, VISITOR);
    abi::emit_branch_if_int_result_zero(emitter, done);
    for (index, offset) in [SCOPE, VISITOR, CONTEXT].into_iter().enumerate() {
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    abi::emit_call_label(emitter, &emitter.target.extern_symbol("__elephc_eval_scope_visit_origins_v1"));
    abi::load_at_offset(emitter, result, SCOPE);
    abi::emit_load_from_address(emitter, result, result, 0);
    abi::store_at_offset(emitter, result, ENTRY);
    emitter.label(scan);
    abi::load_at_offset(emitter, result, ENTRY);
    abi::emit_branch_if_int_result_zero(emitter, done);
    abi::emit_load_from_address(emitter, result, result, 32);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("tbz {result}, #4, {next}"));          // borrowed entries contribute no owning edge
            emitter.instruction(&format!("tbz {result}, #0, {next}"));          // invisible names own no visible scope value
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("test {result}, 16"));                 // only scope-owned cells participate in frame graph traversal
            emitter.instruction(&format!("jz {next}"));                         // borrowed entries contribute no owning edge
            emitter.instruction(&format!("test {result}, 1"));                  // require a currently visible name
            emitter.instruction(&format!("jz {next}"));                         // invisible names contribute no owning edge
        }
    }
    let slot = abi::int_arg_reg_name(emitter.target, 0);
    abi::load_at_offset(emitter, slot, ENTRY);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("add {slot}, {slot}, #24")), // expose this entry's actual Mixed owner field
        Arch::X86_64 => emitter.instruction(&format!("add {slot}, 24")),        // expose this entry's actual Mixed owner field
    }
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 1), 0);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 2), CONTEXT);
    abi::load_at_offset(emitter, scratch, VISITOR);
    abi::emit_call_reg(emitter, scratch);
    emitter.label(next);
    abi::load_at_offset(emitter, result, ENTRY);
    abi::emit_load_from_address(emitter, result, result, 0);
    abi::store_at_offset(emitter, result, ENTRY);
    abi::emit_jump(emitter, scan);
    emitter.label(done);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}
