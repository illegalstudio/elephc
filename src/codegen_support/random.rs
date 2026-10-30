//! Purpose:
//! Publishes the elephc-random global Mt19937 range entry into a late-bound runtime slot.
//!
//! Called from:
//! - The `shuffle` EIR lowerer, immediately before the shared runtime helper.
//!
//! Key details:
//! - Call-site publication keeps programs without `shuffle` free of bridge symbol references,
//!   so a program that never touches the global Mt19937 state links no `libelephc_random.a`.
//!   `array_rand()` reaches the same state through the injected prelude helper instead.
//! - Both supported architectures publish the same single entry.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// The runtime slot the shared `__rt_mt_uniform` helper calls through.
const SLOT: &str = "_elephc_random_mt_uniform_fn";

/// Publishes `elephc_random_mt_uniform` into the shared runtime slot.
pub(crate) fn publish_elephc_random_function_pointers(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            let symbol = emitter
                .target
                .extern_symbol("elephc_random_mt_uniform");
            abi::emit_extern_symbol_address(emitter, "x9", &symbol);
            abi::emit_symbol_address(emitter, "x10", SLOT);
            emitter.instruction("str x9, [x10]");                                   // publish the global Mt19937 range entry into its runtime slot
        }
        Arch::X86_64 => {
            let symbol = emitter
                .target
                .extern_symbol("elephc_random_mt_uniform");
            abi::emit_extern_symbol_address(emitter, "r9", &symbol);
            abi::emit_store_reg_to_symbol(emitter, "r9", SLOT, 0);                  // publish the global Mt19937 range entry into its runtime slot
        }
    }
}
