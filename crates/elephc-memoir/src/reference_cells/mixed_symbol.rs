//! Purpose:
//! Publishes owned Mixed values into stable symbols or their persistent reference children.
//!
//! Called from:
//! - Compiler adapters for direct static stores and retained eval/Reflection setters.
//!
//! Key details:
//! - New storage and initialization state are published before displaced-owner destruction.
//! - The compiler supplies layout, release policy, transfer placement and label identity.

use crate::{abi, Arch, Emitter, FrameOffset, RecordOffset};
use super::mixed_payload::follow_mixed_reference_slot;

/// Host-selected symbol storage and owner transfer policy for one publication.
#[derive(Clone, Copy)]
pub struct Context<'a> {
    /// Stable symbol containing the current Mixed owner or persistent wrapper.
    pub symbol: &'a str,
    /// Offset of host initialization metadata from the symbol base, separate from its payload.
    pub initialization_offset: RecordOffset,
    /// Whether this publication consumes the displaced owner after replacement.
    pub release_previous: bool,
    /// Optional frame slot whose owner transfers into the published storage.
    pub transfer_owner: Option<FrameOffset>,
    /// Caller-unique prefix for the inline wrapper-following branch label.
    pub prefix: &'a str,
}

/// Transfers the current Mixed result into stable storage and optionally retires its previous owner.
pub fn publish(emitter: &mut impl Emitter, context: Context<'_>) {
    let (result, address, previous, marker) = match emitter.arch() {
        Arch::AArch64 => ("x0", "x9", "x10", "x12"),
        Arch::X86_64 => ("rax", "r11", "r10", "r8"),
    };
    abi::emit_push_reg(emitter, result);
    abi::emit_symbol_address(emitter, address, context.symbol);
    follow_mixed_reference_slot(emitter, address, previous, marker, &format!("{}_payload", context.prefix));
    abi::emit_load_from_address(emitter, previous, address, 0);
    abi::emit_store_to_address(emitter, result, address, 0);
    abi::emit_push_reg(emitter, previous);
    if let Some(offset) = context.transfer_owner {
        abi::emit_store_zero_to_local_slot(emitter, offset.bytes());
    }
    abi::emit_store_zero_to_symbol(emitter, context.symbol, context.initialization_offset.bytes());
    abi::emit_pop_reg(emitter, result);
    if context.release_previous { abi::emit_call_label(emitter, "__rt_decref_any"); }
    abi::emit_pop_reg(emitter, result);
}
