//! Purpose:
//! Restores collector controls and reclaims the native heap at a full reset.
//!
//! Called from:
//! - The compiler's full-reset web request lifecycle.
//!
//! Key details:
//! - The caller selects the reset boundary after completing PHP-visible cleanup.
//! - Live-base clearing runs before resetting the bump pointer.

use crate::{abi, Emitter};

/// Restores request-local cycle-collector controls and counters to process-start defaults.
pub fn emit_gc_state_reset(emitter: &mut impl Emitter) {
    emitter.comment("reset cycle collector controls and counters for the next request");
    abi::emit_call_label(emitter, "__rt_gc_drop_pins");
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 1);
    abi::emit_store_reg_to_symbol(
        emitter,
        abi::int_result_reg(emitter),
        "_gc_enabled",
        0,
    );
    for symbol in [
        "_gc_collecting",
        "_gc_pending_throw",
        "_gc_freeing_unreachable",
        "_gc_release_suppressed",
        "_gc_runs",
        "_gc_collected",
        "_gc_safe_point_credit",
        "_gc_safe_point_live_limit",
        "_gc_destructor_depth",
    ] {
        abi::emit_store_zero_to_symbol(emitter, symbol, 0);
    }
}

/// Reclaims the entire arena after the caller has retired its rooted owners.
/// The live-base bitmap still uses the old high-water mark when it is cleared.
pub fn emit_heap_arena_reset(emitter: &mut impl Emitter) {
    emitter.comment("reset the PHP heap arena to pure-bump allocation for the next request");
    crate::heap_live_bases::emit_reset(emitter);
    abi::emit_store_zero_to_symbol(emitter, "_heap_off", 0);
    abi::emit_store_zero_to_symbol(emitter, "_heap_free_list", 0);
    abi::emit_call_label(emitter, "__rt_heap_bins_clear");
    // Bulk reclamation also retires every block that remained after typed cleanup.
    // Keep cumulative allocation totals, but do not carry their live footprint into a new arena.
    let value = abi::int_result_reg(emitter);
    abi::emit_load_symbol_to_reg(emitter, value, "_gc_allocs", 0);
    abi::emit_store_reg_to_symbol(emitter, value, "_gc_frees", 0);
    abi::emit_store_zero_to_symbol(emitter, "_gc_live", 0);
    // `memory_get_peak_usage()` is per request in PHP, so its watermark starts over with the arena.
    abi::emit_store_zero_to_symbol(emitter, "_gc_peak", 0);
}
