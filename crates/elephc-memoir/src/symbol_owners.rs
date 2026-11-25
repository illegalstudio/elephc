//! Purpose:
//! Publishes symbol-backed owner replacements before retiring displaced values.
//!
//! Called from:
//! - Compiler static-property and callable-result storage.
//!
//! Key details:
//! - The replacement is visible before cleanup can run destructors or reenter the same symbol.
//! - Consumers select semantic marker updates and the release helper for their owner type.

use crate::{abi, Emitter};

/// Replaces the owner in `symbol`, preserves the published result, then retires its predecessor.
pub fn emit_replace_result_owner<E, P, R>(
    emitter: &mut E,
    symbol: &str,
    after_publish: P,
    release_previous: R,
)
where
    E: Emitter,
    P: FnOnce(&mut E),
    R: FnOnce(&mut E),
{
    let result = abi::int_result_reg(emitter);
    let previous = abi::secondary_scratch_reg(emitter);
    abi::emit_load_symbol_to_reg(emitter, previous, symbol, 0);
    abi::emit_push_reg(emitter, previous);
    abi::emit_store_reg_to_symbol(emitter, result, symbol, 0);
    abi::emit_push_reg(emitter, result);
    after_publish(emitter);
    emitter.emit_load_temporary_stack_slot(result, 16);
    release_previous(emitter);
    abi::emit_pop_reg(emitter, result);
    emitter.emit_release_temporary_stack(16);
}
