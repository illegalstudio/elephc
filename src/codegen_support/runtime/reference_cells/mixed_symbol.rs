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
    let (result, address, previous, marker) = match emitter.target.arch {
        Arch::AArch64 => ("x0", "x9", "x10", "x12"),
        Arch::X86_64 => ("rax", "r11", "r10", "r8"),
    };
    abi::emit_push_reg(emitter, result);
    abi::emit_symbol_address(emitter, address, symbol);
    follow_mixed_reference_slot(emitter, address, previous, marker, &format!("{prefix}_payload"));
    abi::emit_load_from_address(emitter, previous, address, 0);
    abi::emit_store_to_address(emitter, result, address, 0);
    abi::emit_push_reg(emitter, previous);
    if let Some(offset) = transfer_owner {
        abi::emit_store_zero_to_local_slot(emitter, offset);
    }
    abi::emit_store_zero_to_symbol(emitter, symbol, 8);
    abi::emit_pop_reg(emitter, result);
    if release_previous { abi::emit_call_label(emitter, "__rt_decref_any"); }
    abi::emit_pop_reg(emitter, result);
}
