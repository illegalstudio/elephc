//! Purpose:
//! Supplies compiler-owned static layout and transfer policy to Memoir publication.
//!
//! Called from:
//! - Direct static-property lowering and eval's retained static-property setter.
//!
//! Key details:
//! - Type checks and EIR owner selection stay in the compiler.
//! - Memoir preserves persistent aliases and publishes initialization before displaced release.

use crate::codegen_support::emit::Emitter;
use elephc_memoir::{reference_cells::mixed_symbol::{self, Context}, FrameOffset, RecordOffset};

/// Publishes a Mixed result with the compiler's static initialization layout and owner policy.
pub(crate) fn publish_mixed_symbol(
    emitter: &mut Emitter, symbol: &str, release_previous: bool, transfer_owner: Option<usize>, prefix: &str,
) {
    mixed_symbol::publish(emitter, Context {
        symbol,
        initialization_offset: RecordOffset::new(8),
        release_previous,
        transfer_owner: transfer_owner.map(FrameOffset::new),
        prefix,
    });
}

#[cfg(test)]
mod memoir_tests;
