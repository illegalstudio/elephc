//! Purpose:
//! Publishes boxed static-property values through persistent PHP reference wrappers.
//!
//! Called from:
//! - Native static-property stores and eval's retained static-property setter.
//!
//! Key details:
//! - The symbol remains stable storage; an escaping PHP wrapper owns its child independently.
//! - Initialization is published before the previous value can run a destructor.

use super::*;

/// Transfers the current owned Mixed result into a symbol or its existing PHP reference child.
pub(crate) fn publish_mixed_symbol(
    emitter: &mut Emitter, symbol: &str, release_previous: bool, transfer_owner: Option<usize>, prefix: &str,
) {
    let result = abi::int_result_reg(emitter);
    abi::emit_push_reg(emitter, result);
    swap_mixed_symbol(emitter, symbol, prefix);
    abi::emit_store_zero_to_symbol(emitter, symbol, 8);
    if let Some(offset) = transfer_owner { abi::emit_store_zero_to_local_slot(emitter, offset); }
    if release_previous { abi::emit_call_label(emitter, "__rt_decref_any"); }
    abi::emit_pop_reg(emitter, result);
}

/// Publishes an owned box and returns the displaced owner without running PHP cleanup.
/// An identical wrapper keeps its identity and returns the incoming extra owner for retirement.
pub(crate) fn swap_mixed_symbol(emitter: &mut Emitter, symbol: &str, prefix: &str) {
    let (result, address, previous, marker) = match emitter.target.arch {
        Arch::AArch64 => ("x0", "x9", "x10", "x12"),
        Arch::X86_64 => ("rax", "r11", "r10", "r8"),
    };
    let unchanged = format!("{prefix}_unchanged");
    let done = format!("{prefix}_done");
    abi::emit_symbol_address(emitter, address, symbol);
    abi::emit_load_from_address(emitter, previous, address, 0);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {result}, {previous}"));          // an imported wrapper may already be the program's exact reference
            emitter.instruction(&format!("b.eq {unchanged}"));                  // never install a wrapper as its own child
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {result}, {previous}"));          // an imported wrapper may already be the program's exact reference
            emitter.instruction(&format!("je {unchanged}"));                    // never install a wrapper as its own child
        }
    }
    follow_mixed_reference_slot(emitter, address, previous, marker, &format!("{prefix}_payload"));
    abi::emit_load_from_address(emitter, previous, address, 0);
    abi::emit_store_to_address(emitter, result, address, 0);
    abi::emit_reg_move(emitter, result, previous);
    abi::emit_jump(emitter, &done);
    emitter.label(&unchanged);
    emitter.label(&done);
}
