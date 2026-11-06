//! Purpose:
//! Preserves policies and reference owners when Serializable payloads share a parser registry.
//!
//! Called from:
//! - Unserialize begin/end and builtin lowering for nested calls without explicit options.
//!
//! Key details:
//! - A saved count of -1 marks a shared registry; ordinary logical counts are nonnegative.
//! - Nested shared owners move to the enclosing context before its cleanup list is detached.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits legacy policy inheritance and reference-owner promotion helpers on every target.
pub(super) fn emit(emitter: &mut Emitter) {
    emit_inherit_policy(emitter);
    emit_promote_data(emitter);
}

/// Branches away unless the active context snapshot shares its reference registry.
fn emit_shared_check(emitter: &mut Emitter, done: &str) {
    let result = abi::int_result_reg(emitter);
    abi::emit_load_symbol_to_reg(emitter, result, "_unser_context", 0);
    abi::emit_branch_if_int_result_zero(emitter, done);
    abi::emit_load_from_address(emitter, result, result, 32);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmn x0, #1");                                  // only the negative sharing marker permits legacy inheritance
            emitter.instruction(&format!("b.ne {done}"));                       // ordinary nested calls keep their isolated state
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, -1");                                 // distinguish a shared snapshot from a saved logical count
            emitter.instruction(&format!("jne {done}"));                        // ordinary nested calls need no legacy adaptation
        }
    }
}

/// Inherits the enclosing class policy only when the nested call omitted its options argument.
fn emit_inherit_policy(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    // -- inherit and retain the outer policy for shared calls without explicit options --
    emitter.blank();
    emitter.label_global("__rt_unserialize_inherit_policy");
    abi::emit_frame_prologue(emitter, 32);
    emit_shared_check(emitter, "__rt_unserialize_inherit_policy_done");
    abi::emit_load_symbol_to_reg(emitter, scratch, "_unser_context", 0);
    for (offset, symbol) in [(8, "_unser_allowed_mode"), (24, "_unser_allowed_list_mixed"), (16, "_unser_allowed_list")] {
        abi::emit_load_from_address(emitter, result, scratch, offset);
        abi::emit_store_reg_to_symbol(emitter, result, symbol, 0);
    }
    abi::emit_branch_if_int_result_zero(emitter, "__rt_unserialize_inherit_policy_done");
    abi::emit_call_label(emitter, "__rt_incref");
    emitter.label("__rt_unserialize_inherit_policy_done");
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);
}

/// Keeps nested registry leases alive until the enclosing shared parser completes.
fn emit_promote_data(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    let depth = abi::tertiary_scratch_reg(emitter);
    // -- demote the newest reference leases to the enclosing parser depth --
    emitter.blank();
    emitter.label_global("__rt_unserialize_promote_legacy_data");
    emit_shared_check(emitter, "__rt_unserialize_promote_legacy_done");
    abi::emit_load_symbol_to_reg(emitter, depth, "_unser_active", 0);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("sub {depth}, {depth}, #1")), // promote owners to the enclosing parser depth
        Arch::X86_64 => emitter.instruction(&format!("sub {depth}, 1")),        // promote owners to the enclosing parser depth
    }
    abi::emit_load_symbol_to_reg(emitter, result, "_unser_temporaries", 0);
    emitter.label("__rt_unserialize_promote_legacy_loop");
    abi::emit_branch_if_int_result_zero(emitter, "__rt_unserialize_promote_legacy_done");
    abi::emit_load_from_address(emitter, scratch, result, 16);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {scratch}, {depth}"));            // compare each owner with the enclosing context depth
            emitter.instruction("b.ls __rt_unserialize_promote_legacy_done");   // older owners already belong to the enclosing parser
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {scratch}, {depth}"));            // compare each owner with the enclosing context depth
            emitter.instruction("jbe __rt_unserialize_promote_legacy_done");    // older owners need no promotion
        }
    }
    abi::emit_store_to_address(emitter, depth, result, 16);
    abi::emit_load_from_address(emitter, result, result, 0);
    abi::emit_jump(emitter, "__rt_unserialize_promote_legacy_loop");
    emitter.label("__rt_unserialize_promote_legacy_done");
    abi::emit_return(emitter);
}
